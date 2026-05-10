//! P5-C DP-C4 error type.
//!
//! `SignerError` is `#[non_exhaustive]` + payload-free in Phase 5.
//! `Clone + Copy + PartialEq + Eq` derives let SC-1/SC-2 compare via
//! `assert_eq!(...)` directly without `matches!`. If Phase 6b adds a
//! payload-bearing variant, drop `Copy` and switch tests to `matches!`.

/// All error returns from any [`crate::Signer`] impl in Phase 5 are
/// the [`SignerError::SignerDisabled`] variant. The `Display` text
/// MUST contain the literal phrase "Phase 6b Production Gate" — SC-3
/// pins this as the spec-drift guard.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SignerError {
    #[error("signer disabled — production signing requires Phase 6b Production Gate")]
    SignerDisabled,
}
