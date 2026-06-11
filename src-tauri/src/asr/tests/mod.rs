//! Comprehensive integration tests for the provider-agnostic ASR architecture.
//!
//! These tests verify:
//! - Generic provider selection and registration
//! - Fallback to Groq behavior
//! - Diagnostics privacy (no transcript/audio/key leakage)
//! - Resource policy enforcement
//! - No regressions in bubble, recording, clipboard

pub mod provider_selection;
pub mod fallback_tests;
pub mod policy_tests;
pub mod diagnostics_tests;
pub mod privacy_tests;
