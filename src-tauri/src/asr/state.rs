use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Idle,
    Recording,
    Transcribing,
    Cleaning,
    Pasting,
    Complete,
    Error,
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: String,
    pub phase: SessionPhase,
    pub provider_id: String,
    pub model_id: String,
    pub started_at: Instant,
    pub error: Option<String>,
}

impl SessionState {
    pub fn new(provider_id: String, model_id: String) -> Self {
        Self {
            session_id: uuid_v4(),
            phase: SessionPhase::Idle,
            provider_id,
            model_id,
            started_at: Instant::now(),
            error: None,
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started_at
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }

    pub fn transition(&mut self, phase: SessionPhase) {
        self.phase = phase;
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.phase = SessionPhase::Error;
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeState {
    pub default_provider: String,
    pub experimental_providers: Vec<String>,
    pub disabled_providers: Vec<String>,
}

impl RuntimeState {
    pub fn new(default_provider: String) -> Self {
        Self {
            default_provider,
            experimental_providers: Vec::new(),
            disabled_providers: Vec::new(),
        }
    }

    pub fn with_experimental(mut self, providers: Vec<String>) -> Self {
        self.experimental_providers = providers;
        self
    }

    pub fn with_disabled(mut self, providers: Vec<String>) -> Self {
        self.disabled_providers = providers;
        self
    }
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("sess-{:016x}", nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_initializes_in_idle() {
        let s = SessionState::new("groq".into(), "whisper".into());
        assert_eq!(s.phase, SessionPhase::Idle);
        assert_eq!(s.provider_id, "groq");
        assert_eq!(s.model_id, "whisper");
    }

    #[test]
    fn session_state_transitions() {
        let mut s = SessionState::new("groq".into(), "whisper".into());
        s.transition(SessionPhase::Transcribing);
        assert_eq!(s.phase, SessionPhase::Transcribing);
    }

    #[test]
    fn session_state_error() {
        let mut s = SessionState::new("groq".into(), "whisper".into());
        s.set_error("something broke".into());
        assert_eq!(s.phase, SessionPhase::Error);
        assert_eq!(s.error.as_deref(), Some("something broke"));
    }

    #[test]
    fn session_elapsed_ms_increases() {
        let s = SessionState::new("groq".into(), "whisper".into());
        let first = s.elapsed_ms();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = s.elapsed_ms();
        assert!(second >= first);
    }

    #[test]
    fn runtime_state_constructs() {
        let r = RuntimeState::new("groq".into())
            .with_experimental(vec!["whisper_local".into()])
            .with_disabled(vec!["old_provider".into()]);
        assert_eq!(r.default_provider, "groq");
        assert_eq!(r.experimental_providers, vec!["whisper_local"]);
        assert_eq!(r.disabled_providers, vec!["old_provider"]);
    }

    #[test]
    fn session_id_is_unique() {
        let a = SessionState::new("groq".into(), "whisper".into());
        let b = SessionState::new("groq".into(), "whisper".into());
        assert_ne!(a.session_id, b.session_id);
    }
}
