use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Holds the current active pipeline trace_id, generated when a recording session starts.
/// Reset on each new `start_recording`. Commands read from this to correlate their log events.
#[derive(Debug)]
pub struct PipelineContext {
    current: Mutex<Option<String>>,
}

impl PipelineContext {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(None),
        }
    }

    /// Generate a new 8-char hex trace_id and store it as the current session.
    pub fn start_session(&self) -> String {
        let id = generate_trace_id();
        if let Ok(mut guard) = self.current.lock() {
            *guard = Some(id.clone());
        }
        id
    }

    /// Return the current trace_id if one exists.
    pub fn current_trace_id(&self) -> Option<String> {
        self.current.lock().ok().and_then(|g| g.clone())
    }

    /// Clear the current session.
    pub fn end_session(&self) {
        if let Ok(mut guard) = self.current.lock() {
            *guard = None;
        }
    }
}

fn generate_trace_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // Use lower 32 bits of nanosecond timestamp as hex
    format!("{:08x}", (nanos & 0xFFFF_FFFF) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_session_returns_8_hex_chars() {
        let ctx = PipelineContext::new();
        let id = ctx.start_session();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn current_trace_id_returns_stored_id() {
        let ctx = PipelineContext::new();
        assert!(ctx.current_trace_id().is_none());
        let id = ctx.start_session();
        assert_eq!(ctx.current_trace_id().unwrap(), id);
    }

    #[test]
    fn end_session_clears_id() {
        let ctx = PipelineContext::new();
        ctx.start_session();
        assert!(ctx.current_trace_id().is_some());
        ctx.end_session();
        assert!(ctx.current_trace_id().is_none());
    }

    #[test]
    fn successive_sessions_have_different_ids() {
        let ctx = PipelineContext::new();
        let id1 = ctx.start_session();
        ctx.end_session();
        let id2 = ctx.start_session();
        assert_ne!(id1, id2);
    }

    #[test]
    fn start_session_replaces_previous_id() {
        let ctx = PipelineContext::new();
        let id1 = ctx.start_session();
        let id2 = ctx.start_session();
        assert_eq!(ctx.current_trace_id().unwrap(), id2);
        assert_ne!(id1, id2);
    }
}
