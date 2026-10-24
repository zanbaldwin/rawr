//! Paying for narration: the async loop that turns missing batches into recordings.
//!
//! Sections run at the same time, up to the provider's concurrency. The
//! batches of one section run in order, so each request can refer to the
//! ones before it with the same voice (continuity). Progress is reported as a stream of
//! [`NarrationEvent`]s.

use crate::plan::{BatchState, Plan, PlannedBatch, SectionPlan};
use crate::store::{Recording, RecordingStatus, Store};
use futures::stream::BoxStream;
use futures::{StreamExt, stream};
use rawr_source::Rating;
use rawr_tts::error::ErrorKind as TtsErrorKind;
use rawr_tts::{Continuity, Cost, SynthesisRequest, SynthesiserHandle};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;

/// How to narrate.
#[derive(Debug, Clone)]
pub struct NarrationOptions {
    /// Requests at the same time. `None` uses the provider's default.
    pub concurrency: Option<usize>,
    /// Stop before sending more than this many characters in this run.
    pub max_chars: Option<usize>,
    /// Pay again for batches whose earlier recording had an alignment mismatch.
    pub retry_mismatched: bool,
    /// Explicit approval to send a document whose rating needs it ([`Plan::needs_approval`]).
    pub approved: bool,
    /// Attempts for one batch, when the error is retryable.
    pub max_attempts: u32,
    /// Delay before the first retry; it doubles for each further retry.
    pub base_delay: Duration,
    /// Cancel to stop the run gracefully (for example on Ctrl-C). No new
    /// request starts. Requests in flight finish and are stored, because
    /// they are paid for.
    pub stop: Option<CancellationToken>,
    /// Pay only for these sections ([`Section::number`](rawr_source::Section::number)).
    /// `None` pays for all. The ledger always lists the whole plan.
    pub sections: Option<Vec<u32>>,
}

impl Default for NarrationOptions {
    fn default() -> Self {
        Self {
            concurrency: None,
            max_chars: None,
            retry_mismatched: false,
            approved: false,
            max_attempts: 4,
            base_delay: Duration::from_secs(5),
            stop: None,
            sections: None,
        }
    }
}

/// Progress of a narration run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NarrationEvent {
    /// The ledger is written; this many batches and characters are to be paid for.
    Started { batches: usize, chars: usize },
    /// A batch was paid for and stored.
    Recorded {
        section: u32,
        batch: usize,
        chars: usize,
        cost: Option<Cost>,
    },
    /// A retryable error; the batch is tried again after `delay`.
    Retrying {
        section: u32,
        batch: usize,
        attempt: u32,
        delay: Duration,
        error: TtsErrorKind,
    },
    /// A batch was paid for, but its alignment did not match the text. It is
    /// stored with that status and never paid for again without
    /// [`NarrationOptions::retry_mismatched`]. Its section stops.
    Mismatched {
        section: u32,
        batch: usize,
        chars: usize,
        cost: Option<Cost>,
    },
    /// The section stopped; other sections continue.
    SectionFailed {
        section: u32,
        batch: usize,
        error: TtsErrorKind,
    },
    /// The whole run stopped: no further requests are sent.
    Stopped { reason: StopReason },
    /// Always the last event.
    Finished { chars: usize, spent: Vec<Cost> },
}

/// Why a run stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// The document's rating needs approval ([`Plan::needs_approval`]), and
    /// [`NarrationOptions::approved`] is not set. Nothing was sent or written.
    NeedsApproval(Rating),
    /// The next batch would go over [`NarrationOptions::max_chars`].
    Budget,
    /// The provider refused the account or key (quota, credentials).
    Provider(TtsErrorKind),
    /// [`NarrationOptions::stop`] was cancelled.
    Cancelled,
    /// A recording or the ledger could not be stored. Nothing more is paid
    /// for while recordings cannot be kept.
    Storage,
}

/// Narrate the missing batches of a plan.
///
/// A document that needs approval, without [`NarrationOptions::approved`],
/// stops before anything is sent or written. Otherwise the ledger is written
/// first. Each recording is stored before the next
/// request of its section is sent. The stream is boxed, so callers can use
/// `next()` on it without pinning it.
pub fn narrate<'a>(
    plan: &'a Plan,
    synthesiser: SynthesiserHandle,
    store: &'a Store,
    options: NarrationOptions,
) -> BoxStream<'a, NarrationEvent> {
    let (sender, receiver) = futures::channel::mpsc::unbounded();
    let run = Run {
        plan,
        synthesiser,
        store,
        options,
        events: sender,
        stopped: AtomicBool::new(false),
        sent: AtomicUsize::new(0),
        spent: Mutex::new(Vec::new()),
    };
    let driver = async move { run.run().await };
    stream::select(receiver.map(Some), stream::once(driver).map(|()| None)).filter_map(std::future::ready).boxed()
}

struct Run<'a> {
    plan: &'a Plan,
    synthesiser: SynthesiserHandle,
    store: &'a Store,
    options: NarrationOptions,
    events: futures::channel::mpsc::UnboundedSender<NarrationEvent>,
    stopped: AtomicBool,
    /// Characters reserved for requests: sent and recorded, or in flight.
    sent: AtomicUsize,
    spent: Mutex<Vec<Cost>>,
}

impl Run<'_> {
    async fn run(self) {
        if self.plan.needs_approval && !self.options.approved {
            self.stop(StopReason::NeedsApproval(self.plan.rating));
        } else {
            self.pay().await;
        }
        let spent = self.spent.lock().expect("spent lock").clone();
        self.emit(NarrationEvent::Finished {
            chars: self.sent.load(Ordering::SeqCst),
            spent,
        });
    }

    async fn pay(&self) {
        let mut ledger = self.plan.ledger(OffsetDateTime::now_utc());
        let language = self.plan.language.as_deref();
        let previous = self.store.ledger(&self.plan.identifier).await.ok().flatten();
        ledger.voice_name = match self.synthesiser.voice_name(language).await {
            Ok(Some(name)) => Some(name),
            _ => previous.as_ref().and_then(|previous| previous.voice_name.clone()),
        };
        for role in self.plan.roles() {
            let name = match self.synthesiser.role_voice_name(language, Some(role)).await {
                Ok(Some(name)) => Some(name),
                _ => previous.as_ref().and_then(|previous| previous.voices.get(role).cloned()),
            };
            if let Some(name) = name {
                ledger.voices.insert(role.to_string(), name);
            }
        }
        if self.store.write_ledger(&ledger).await.is_err() {
            self.stop(StopReason::Storage);
        } else {
            let to_pay = self
                .plan
                .sections
                .iter()
                .filter(|section| self.included(section))
                .flat_map(|section| &section.batches)
                .filter(|batch| self.to_pay(batch));
            let (batches, chars) = to_pay.fold((0, 0), |(count, chars), batch| (count + 1, chars + batch.chars));
            self.emit(NarrationEvent::Started { batches, chars });
            let concurrency = self.options.concurrency.unwrap_or(self.synthesiser.capabilities().concurrency).max(1);
            let sections: Vec<_> = self
                .plan
                .sections
                .iter()
                .filter(|section| self.included(section))
                .map(|section| self.section(section))
                .collect();
            stream::iter(sections).buffer_unordered(concurrency).collect::<Vec<()>>().await;
        }
    }

    fn included(&self, section: &SectionPlan) -> bool {
        self.options.sections.as_ref().is_none_or(|sections| sections.contains(&section.script.section))
    }

    fn to_pay(&self, batch: &PlannedBatch) -> bool {
        match batch.state {
            BatchState::Missing => true,
            BatchState::Mismatched => self.options.retry_mismatched,
            BatchState::Recorded => false,
        }
    }

    async fn section(&self, section: &SectionPlan) {
        let number = section.script.section;
        let mut previous: Vec<(String, OffsetDateTime, Option<String>)> = Vec::new();
        for (index, batch) in section.batches.iter().enumerate() {
            if self.stopped.load(Ordering::SeqCst) {
                return;
            }
            let existing = match self.store.meta(&batch.key).await {
                Ok(existing) => existing,
                Err(_) => return self.stop(StopReason::Storage),
            };
            if let Some(meta) = existing
                && (meta.status == RecordingStatus::Ok || !self.options.retry_mismatched)
            {
                if let Some(request_id) = meta.receipt.request_id {
                    previous.push((request_id, meta.created_at, batch.voice.clone()));
                }
                continue;
            }
            let continuity = self.continuity(&previous, batch, section.batches.get(index + 1));
            let request = SynthesisRequest {
                text: batch.text.clone(),
                language: self.plan.language.clone(),
                role: batch.voice.clone(),
                seed: Some(section.script.seed),
                continuity: continuity.clone(),
            };
            let Some(outcome) = self.request(number, index, batch, &request).await else {
                return;
            };
            let (synthesis, status) = match outcome {
                Ok(synthesis) => (synthesis, RecordingStatus::Ok),
                Err(synthesis) => (synthesis, RecordingStatus::AlignmentMismatch),
            };
            let cost = synthesis.receipt.cost;
            let request_id = synthesis.receipt.request_id.clone();
            let created_at = OffsetDateTime::now_utc();
            let inputs = self.plan.key_inputs(section, batch);
            let recording = Recording::new(&inputs, synthesis, &continuity, status, created_at);
            if self.store.write(&recording).await.is_err() {
                return self.stop(StopReason::Storage);
            }
            if let Some(cost) = cost {
                self.add_cost(cost);
            }
            if status == RecordingStatus::AlignmentMismatch {
                self.emit(NarrationEvent::Mismatched {
                    section: number,
                    batch: index,
                    chars: batch.chars,
                    cost,
                });
                return;
            }
            self.emit(NarrationEvent::Recorded {
                section: number,
                batch: index,
                chars: batch.chars,
                cost,
            });
            if let Some(request_id) = request_id {
                previous.push((request_id, created_at, batch.voice.clone()));
            }
        }
    }

    /// Send one batch, with retries. `None` means the section (or the run) stops.
    /// `Some(Err(_))` is a paid synthesis whose alignment did not match.
    async fn request(
        &self,
        section: u32,
        batch: usize,
        planned: &PlannedBatch,
        request: &SynthesisRequest,
    ) -> Option<Result<rawr_tts::Synthesis, rawr_tts::Synthesis>> {
        let mut attempt = 1;
        loop {
            if self.stopped.load(Ordering::SeqCst) {
                return None;
            }
            if self.options.stop.as_ref().is_some_and(CancellationToken::is_cancelled) {
                self.stop(StopReason::Cancelled);
                return None;
            }
            if !self.reserve(planned.chars) {
                self.stop(StopReason::Budget);
                return None;
            }
            let error = match self.synthesiser.synthesise(request).await {
                Ok(synthesis) => return Some(Ok(synthesis)),
                Err(error) => (*error).clone(),
            };
            if let TtsErrorKind::AlignmentMismatch(synthesis) = error {
                return Some(Err(*synthesis));
            }
            self.sent.fetch_sub(planned.chars, Ordering::SeqCst);
            match error {
                TtsErrorKind::QuotaExceeded | TtsErrorKind::Unauthorised => {
                    self.stop(StopReason::Provider(error));
                    return None;
                },
                error if error.is_retryable() && attempt < self.options.max_attempts => {
                    let delay = self.options.base_delay * 2u32.pow(attempt - 1);
                    self.emit(NarrationEvent::Retrying { section, batch, attempt, delay, error });
                    match &self.options.stop {
                        Some(stop) => {
                            let sleep = std::pin::pin!(tokio::time::sleep(delay));
                            let cancelled = std::pin::pin!(stop.cancelled());
                            futures::future::select(sleep, cancelled).await;
                        },
                        None => tokio::time::sleep(delay).await,
                    }
                    attempt += 1;
                },
                error => {
                    self.emit(NarrationEvent::SectionFailed { section, batch, error });
                    return None;
                },
            }
        }
    }

    /// Continuity for a batch: only earlier recordings and the next batch with the same voice.
    fn continuity(
        &self,
        previous: &[(String, OffsetDateTime, Option<String>)],
        batch: &PlannedBatch,
        next: Option<&PlannedBatch>,
    ) -> Continuity {
        let Some(limits) = self.synthesiser.capabilities().continuity else {
            return Continuity::default();
        };
        let now = OffsetDateTime::now_utc();
        let recent: Vec<String> = previous
            .iter()
            .filter(|(_, created_at, voice)| {
                *voice == batch.voice && (now - *created_at).unsigned_abs() < limits.max_age
            })
            .map(|(request_id, _, _)| request_id.clone())
            .collect();
        Continuity {
            previous_request_ids: recent[recent.len().saturating_sub(limits.max_previous)..].to_vec(),
            next_text: next
                .filter(|next| next.voice == batch.voice)
                .map(|next| next.text.chars().take(limits.max_next_chars).collect()),
        }
    }

    fn reserve(&self, chars: usize) -> bool {
        let Some(limit) = self.options.max_chars else {
            self.sent.fetch_add(chars, Ordering::SeqCst);
            return true;
        };
        let mut sent = self.sent.load(Ordering::SeqCst);
        loop {
            if sent + chars > limit {
                return false;
            }
            match self.sent.compare_exchange(sent, sent + chars, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => return true,
                Err(actual) => sent = actual,
            }
        }
    }

    fn add_cost(&self, cost: Cost) {
        let mut spent = self.spent.lock().expect("spent lock");
        match spent.iter_mut().find(|total| total.unit == cost.unit) {
            Some(total) => total.amount += cost.amount,
            None => spent.push(cost),
        }
    }

    fn stop(&self, reason: StopReason) {
        if !self.stopped.swap(true, Ordering::SeqCst) {
            self.emit(NarrationEvent::Stopped { reason });
        }
    }

    fn emit(&self, event: NarrationEvent) {
        let _ = self.events.unbounded_send(event);
    }
}
