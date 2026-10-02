//! Paying for narration: the async loop that turns missing batches into recordings.
//!
//! Sections run at the same time, up to the provider's concurrency. The
//! batches of one section run in order, so each request can refer to the
//! ones before it (continuity). Progress is reported as a stream of
//! [`NarrationEvent`]s.

use crate::plan::{BatchState, Plan, PlannedBatch, SectionPlan};
use crate::store::{Recording, RecordingStatus, Store};
use futures::stream::BoxStream;
use futures::{StreamExt, stream};
use rawr_tts::error::ErrorKind as TtsErrorKind;
use rawr_tts::{Continuity, Cost, SynthesisRequest, SynthesiserHandle};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use time::OffsetDateTime;

/// How to narrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NarrationOptions {
    /// Requests at the same time. `None` uses the provider's default.
    pub concurrency: Option<usize>,
    /// Stop before sending more than this many characters in this run.
    pub max_chars: Option<usize>,
    /// Pay again for batches whose earlier recording had an alignment mismatch.
    pub retry_mismatched: bool,
    /// Attempts for one batch, when the error is retryable.
    pub max_attempts: u32,
    /// Delay before the first retry; it doubles for each further retry.
    pub base_delay: Duration,
}

impl Default for NarrationOptions {
    fn default() -> Self {
        Self {
            concurrency: None,
            max_chars: None,
            retry_mismatched: false,
            max_attempts: 4,
            base_delay: Duration::from_secs(5),
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
    /// The next batch would go over [`NarrationOptions::max_chars`].
    Budget,
    /// The provider refused the account or key (quota, credentials).
    Provider(TtsErrorKind),
    /// A recording or the ledger could not be stored. Nothing more is paid
    /// for while recordings cannot be kept.
    Storage,
}

/// Narrate the missing batches of a plan.
///
/// The ledger is written first. Each recording is stored before the next
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
        let mut ledger = self.plan.ledger(OffsetDateTime::now_utc());
        ledger.voice_name = match self.synthesiser.voice_name(self.plan.language.as_deref()).await {
            Ok(Some(name)) => Some(name),
            _ => self.store.ledger(&self.plan.identifier).await.ok().flatten().and_then(|previous| previous.voice_name),
        };
        if self.store.write_ledger(&ledger).await.is_err() {
            self.stop(StopReason::Storage);
        } else {
            let to_pay = self.plan.batches().filter(|batch| self.to_pay(batch));
            let (batches, chars) = to_pay.fold((0, 0), |(count, chars), batch| (count + 1, chars + batch.chars));
            self.emit(NarrationEvent::Started { batches, chars });
            let concurrency = self.options.concurrency.unwrap_or(self.synthesiser.capabilities().concurrency).max(1);
            let sections: Vec<_> = self.plan.sections.iter().map(|section| self.section(section)).collect();
            stream::iter(sections).buffer_unordered(concurrency).collect::<Vec<()>>().await;
        }
        let spent = self.spent.lock().expect("spent lock").clone();
        self.emit(NarrationEvent::Finished {
            chars: self.sent.load(Ordering::SeqCst),
            spent,
        });
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
        let mut previous: Vec<(String, OffsetDateTime)> = Vec::new();
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
                    previous.push((request_id, meta.created_at));
                }
                continue;
            }
            let continuity = self.continuity(&previous, section.batches.get(index + 1));
            let request = SynthesisRequest {
                text: batch.text.clone(),
                language: self.plan.language.clone(),
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
                previous.push((request_id, created_at));
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
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                },
                error => {
                    self.emit(NarrationEvent::SectionFailed { section, batch, error });
                    return None;
                },
            }
        }
    }

    fn continuity(&self, previous: &[(String, OffsetDateTime)], next: Option<&PlannedBatch>) -> Continuity {
        let Some(limits) = self.synthesiser.capabilities().continuity else {
            return Continuity::default();
        };
        let now = OffsetDateTime::now_utc();
        let recent: Vec<String> = previous
            .iter()
            .filter(|(_, created_at)| (now - *created_at).unsigned_abs() < limits.max_age)
            .map(|(request_id, _)| request_id.clone())
            .collect();
        Continuity {
            previous_request_ids: recent[recent.len().saturating_sub(limits.max_previous)..].to_vec(),
            next_text: next.map(|batch| batch.text.chars().take(limits.max_next_chars).collect()),
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
