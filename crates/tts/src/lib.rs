//! Text to speech.
//!
//! [`Synthesiser`] is the one interface every provider implements. Providers
//! differ in what they support (alignment detail, request stitching, cost
//! units), so each one describes itself with [`Capabilities`] and the
//! narration pipeline adapts, instead of using only what all providers share.

#[cfg(feature = "elevenlabs")]
pub mod elevenlabs;
pub mod error;
#[cfg(feature = "fake")]
mod fake;
mod synthesiser;
mod types;

#[cfg(feature = "fake")]
pub use crate::fake::FakeSynthesiser;
pub use crate::synthesiser::{Synthesiser, SynthesiserHandle};
pub use crate::types::{
    Alignment, AlignmentKind, Audio, AudioFormat, Capabilities, CharacterTiming, Continuity, ContinuityLimits, Cost,
    CostUnit, Fingerprint, Receipt, Synthesis, SynthesisRequest,
};
