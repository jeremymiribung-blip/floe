//! Comprehensive integration tests for the provider-agnostic ASR architecture.
//!
//! These tests verify:
//! - Generic provider selection and registration
//! - Fallback behavior
//! - Diagnostics privacy (no transcript/audio/key leakage)
//! - Resource policy enforcement
//! - No regressions in bubble, recording, clipboard

pub mod diagnostics_tests;
pub mod fallback_tests;
pub mod policy_tests;
pub mod privacy_tests;
pub mod provider_selection;
