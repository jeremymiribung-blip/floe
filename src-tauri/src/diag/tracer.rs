use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;

/// A record of one completed (or failed) pipeline run.
#[derive(Debug, Clone, Serialize)]
pub struct PipelineTrace {
    pub trace_id: String,
    pub stages: Vec<StageRecord>,
    pub error: Option<String>,
    pub created_at: String,
    pub total_ms: u64,
}

/// Timing and outcome for a single pipeline stage.
#[derive(Debug, Clone, Serialize)]
pub struct StageRecord {
    pub stage: String,
    pub duration_ms: u64,
    pub success: bool,
    pub metadata: Vec<(String, String)>,
}

/// In-memory circular buffer of recent pipeline traces.
/// Default capacity is 20 traces. Oldest entries are evicted when full.
#[derive(Debug)]
pub struct PipelineTracer {
    traces: Mutex<VecDeque<PipelineTrace>>,
    max_traces: usize,
}

impl PipelineTracer {
    pub fn new(max_traces: usize) -> Self {
        Self {
            traces: Mutex::new(VecDeque::with_capacity(max_traces + 1)),
            max_traces,
        }
    }

    #[allow(dead_code)]
    pub fn push(&self, trace: PipelineTrace) {
        if let Ok(mut traces) = self.traces.lock() {
            if traces.len() >= self.max_traces {
                traces.pop_front();
            }
            traces.push_back(trace);
        }
    }

    pub fn recent(&self, count: usize) -> Vec<PipelineTrace> {
        let count = count.min(self.max_traces).max(1);
        self.traces
            .lock()
            .ok()
            .map(|traces| traces.iter().rev().take(count).cloned().collect())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn clear(&self) {
        if let Ok(mut traces) = self.traces.lock() {
            traces.clear();
        }
    }
}

#[allow(dead_code)]
fn iso_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let millis = d.subsec_millis();
    // Compute date/time from secs since epoch (UTC)
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Simple date from days since epoch (proleptic Gregorian)
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let month_days = if is_leap(y) {
        &LEAP_MONTH_DAYS[..]
    } else {
        &NORMAL_MONTH_DAYS[..]
    };
    let mut m = 1;
    for &md in month_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        m += 1;
    }
    let d = remaining + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y, m, d, hours, minutes, seconds, millis
    )
}

#[allow(dead_code)]
const NORMAL_MONTH_DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
#[allow(dead_code)]
const LEAP_MONTH_DAYS: [i64; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
#[allow(dead_code)]
fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[allow(dead_code)]
pub fn build_trace(
    trace_id: String,
    stages: Vec<StageRecord>,
    error: Option<String>,
    total_ms: u64,
) -> PipelineTrace {
    PipelineTrace {
        trace_id,
        stages,
        error,
        created_at: iso_now(),
        total_ms,
    }
}

#[allow(dead_code)]
pub fn build_stage(
    stage: impl Into<String>,
    duration_ms: u64,
    success: bool,
    metadata: Vec<(String, String)>,
) -> StageRecord {
    StageRecord {
        stage: stage.into(),
        duration_ms,
        success,
        metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracer_starts_empty() {
        let t = PipelineTracer::new(20);
        assert!(t.recent(1).is_empty());
    }

    #[test]
    fn tracer_holds_up_to_max() {
        let t = PipelineTracer::new(3);
        for i in 0..5 {
            t.push(PipelineTrace {
                trace_id: format!("id{i}"),
                stages: vec![],
                error: None,
                created_at: "t".into(),
                total_ms: 100,
            });
        }
        let recent = t.recent(10);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].trace_id, "id4");
        assert_eq!(recent[2].trace_id, "id2");
    }

    #[test]
    fn clear_empties_tracer() {
        let t = PipelineTracer::new(20);
        t.push(PipelineTrace {
            trace_id: "x".into(),
            stages: vec![],
            error: None,
            created_at: "t".into(),
            total_ms: 100,
        });
        t.clear();
        assert!(t.recent(1).is_empty());
    }

    #[test]
    fn recent_respects_count() {
        let t = PipelineTracer::new(20);
        for i in 0..5 {
            t.push(PipelineTrace {
                trace_id: format!("id{i}"),
                stages: vec![],
                error: None,
                created_at: "t".into(),
                total_ms: 100,
            });
        }
        assert_eq!(t.recent(3).len(), 3);
        assert_eq!(t.recent(1).len(), 1);
    }

    #[test]
    fn build_stage_creates_record() {
        let s = build_stage("stt", 1500, true, vec![("model".into(), "whisper".into())]);
        assert_eq!(s.stage, "stt");
        assert_eq!(s.duration_ms, 1500);
        assert!(s.success);
        assert_eq!(s.metadata, vec![("model".into(), "whisper".into())]);
    }

    #[test]
    fn build_trace_creates_pipeline_trace() {
        let stages = vec![build_stage("recording", 500, true, vec![])];
        let trace = build_trace("abc".into(), stages, None, 5000);
        assert_eq!(trace.trace_id, "abc");
        assert_eq!(trace.stages.len(), 1);
        assert!(trace.error.is_none());
        assert_eq!(trace.total_ms, 5000);
    }
}
