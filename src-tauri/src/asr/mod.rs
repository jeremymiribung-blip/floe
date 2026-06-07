#![allow(dead_code)]

use std::{
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub mod mock_sidecar;

const LOCAL_ASR_MODEL: &str = "mock-local-asr";
const SIDE_CAR_ARG: &str = "--floe-mock-asr-sidecar";
const SIDE_CAR_PORT_PREFIX: &str = "FLOE_MOCK_ASR_PORT=";
const SCENARIO_ENV: &str = "FLOE_MOCK_ASR_SCENARIO";
const DISABLE_SIDECAR_ENV: &str = "FLOE_MOCK_ASR_DISABLE_SIDECAR";
const HEARTBEAT_UNHEALTHY_AFTER: Duration = Duration::from_millis(2_500);
const FINAL_WAIT_TIMEOUT: Duration = Duration::from_millis(1_200);
const SIDECAR_START_TIMEOUT: Duration = Duration::from_millis(2_000);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AsrPipelineMode {
    #[default]
    GroqCloud,
    ExperimentalNemotronStreaming,
}

pub type PipelineMode = AsrPipelineMode;

pub fn configured_pipeline_mode() -> AsrPipelineMode {
    pipeline_mode_for_nemotron_flag(crate::experiments::nemotron_streaming_stt_enabled())
}

pub(crate) fn pipeline_mode_for_nemotron_flag(enabled: bool) -> AsrPipelineMode {
    if enabled {
        AsrPipelineMode::ExperimentalNemotronStreaming
    } else {
        AsrPipelineMode::GroqCloud
    }
}

pub fn maybe_run_mock_asr_sidecar_from_args() -> bool {
    let mut args = std::env::args().skip(1);
    let Some(first) = args.next() else {
        return false;
    };

    if first != SIDE_CAR_ARG {
        return false;
    }

    let Some(token) = args.next() else {
        return true;
    };

    let scenario = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or_default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();

    if let Ok(runtime) = runtime {
        let _ = runtime.block_on(mock_sidecar::run_mock_sidecar(token, scenario));
    }

    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MockAsrScenario {
    #[default]
    Success,
    SlowFinal,
    NoHeartbeat,
    CrashDisconnect,
    MalformedEvent,
    ModelMissing,
    Busy,
    Timeout,
}

impl std::str::FromStr for MockAsrScenario {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "success" => Ok(Self::Success),
            "slow_final" | "slow-final" => Ok(Self::SlowFinal),
            "no_heartbeat" | "no-heartbeat" => Ok(Self::NoHeartbeat),
            "crash_disconnect" | "crash-disconnect" | "disconnect" => Ok(Self::CrashDisconnect),
            "malformed_event" | "malformed-event" => Ok(Self::MalformedEvent),
            "model_missing" | "model-missing" => Ok(Self::ModelMissing),
            "busy" => Ok(Self::Busy),
            "timeout" => Ok(Self::Timeout),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for MockAsrScenario {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Success => "success",
            Self::SlowFinal => "slow_final",
            Self::NoHeartbeat => "no_heartbeat",
            Self::CrashDisconnect => "crash_disconnect",
            Self::MalformedEvent => "malformed_event",
            Self::ModelMissing => "model_missing",
            Self::Busy => "busy",
            Self::Timeout => "timeout",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone)]
pub struct PcmAudioChunk {
    pub timestamp_ms: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalAsrDiagnostics {
    pub pipeline_mode: AsrPipelineMode,
    pub local_asr_enabled: bool,
    pub local_asr_available: bool,
    pub sidecar_connected: bool,
    pub sidecar_start_ms: u64,
    pub local_asr_session_ms: u64,
    pub local_asr_final_wait_ms: u64,
    pub local_asr_error_code: Option<String>,
    pub fallback_to_groq_used: bool,
    pub fallback_reason: Option<String>,
}

impl LocalAsrDiagnostics {
    pub fn groq_cloud() -> Self {
        Self {
            pipeline_mode: AsrPipelineMode::GroqCloud,
            local_asr_enabled: false,
            local_asr_available: false,
            sidecar_connected: false,
            sidecar_start_ms: 0,
            local_asr_session_ms: 0,
            local_asr_final_wait_ms: 0,
            local_asr_error_code: None,
            fallback_to_groq_used: false,
            fallback_reason: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAsrFinal {
    pub text: String,
    pub diagnostics: LocalAsrDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAsrFailure {
    pub diagnostics: LocalAsrDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalAsrOutcome {
    Final(LocalAsrFinal),
    Fallback(LocalAsrFailure),
}

impl LocalAsrOutcome {
    pub fn diagnostics(&self) -> LocalAsrDiagnostics {
        match self {
            Self::Final(final_result) => final_result.diagnostics.clone(),
            Self::Fallback(failure) => failure.diagnostics.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    StartSession {
        sample_rate: u32,
        channels: u16,
        format: String,
        language: Option<String>,
        session_id: String,
    },
    EndOfAudio,
    CancelSession,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalAsrEvent {
    Ready,
    PartialTranscript { text: String, stable: bool },
    FinalTranscript { text: String, stable: bool },
    Error { code: String },
    Heartbeat,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LocalAsrErrorCode {
    Disabled,
    SidecarUnavailable,
    SidecarStartTimeout,
    ConnectFailed,
    StartTimeout,
    Busy,
    ModelMissing,
    HeartbeatTimeout,
    FinalTimeout,
    Disconnected,
    MalformedEvent,
    Protocol,
    Internal,
}

impl LocalAsrErrorCode {
    pub fn as_safe_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::SidecarUnavailable => "sidecar_unavailable",
            Self::SidecarStartTimeout => "sidecar_start_timeout",
            Self::ConnectFailed => "connect_failed",
            Self::StartTimeout => "start_timeout",
            Self::Busy => "busy",
            Self::ModelMissing => "model_missing",
            Self::HeartbeatTimeout => "heartbeat_timeout",
            Self::FinalTimeout => "final_timeout",
            Self::Disconnected => "disconnected",
            Self::MalformedEvent => "malformed_event",
            Self::Protocol => "protocol",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LocalAsrError {
    pub code: LocalAsrErrorCode,
}

impl LocalAsrError {
    fn new(code: LocalAsrErrorCode) -> Self {
        Self { code }
    }
}

pub(crate) enum SessionCommand {
    Chunk(PcmAudioChunk),
    End,
    Cancel,
}

#[derive(Default)]
struct SessionSharedState {
    final_text: Option<String>,
    error_code: Option<LocalAsrErrorCode>,
    last_heartbeat_at: Option<Instant>,
    connected: bool,
}

pub struct LocalAsrSession {
    id: String,
    started_at: Instant,
    sidecar_start_ms: u64,
    command_tx: mpsc::UnboundedSender<SessionCommand>,
    state: Arc<Mutex<SessionSharedState>>,
    child: Arc<Mutex<Option<Child>>>,
}

impl LocalAsrSession {
    fn chunk_sender(&self) -> mpsc::UnboundedSender<SessionCommand> {
        self.command_tx.clone()
    }

    async fn finish(&self) -> LocalAsrOutcome {
        let wait_started = Instant::now();
        let _ = self.command_tx.send(SessionCommand::End);

        loop {
            let snapshot = self.snapshot();
            if let Some(text) = snapshot.final_text {
                self.cleanup_child().await;
                return LocalAsrOutcome::Final(LocalAsrFinal {
                    text,
                    diagnostics: self.success_diagnostics(wait_started),
                });
            }

            if let Some(code) = snapshot.error_code {
                self.cleanup_child().await;
                return LocalAsrOutcome::Fallback(LocalAsrFailure {
                    diagnostics: self.failure_diagnostics(code, wait_started),
                });
            }

            if wait_started.elapsed() >= FINAL_WAIT_TIMEOUT {
                self.mark_error(LocalAsrErrorCode::FinalTimeout);
                self.cleanup_child().await;
                return LocalAsrOutcome::Fallback(LocalAsrFailure {
                    diagnostics: self
                        .failure_diagnostics(LocalAsrErrorCode::FinalTimeout, wait_started),
                });
            }

            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    fn cancel(&self) {
        let _ = self.command_tx.send(SessionCommand::Cancel);
    }

    fn mark_error(&self, code: LocalAsrErrorCode) {
        if let Ok(mut state) = self.state.lock() {
            if state.error_code.is_none() && state.final_text.is_none() {
                state.error_code = Some(code);
            }
        }
    }

    fn snapshot(&self) -> SessionSnapshot {
        self.state
            .lock()
            .map(|state| SessionSnapshot {
                final_text: state.final_text.clone(),
                error_code: state.error_code.clone(),
                connected: state.connected,
            })
            .unwrap_or_default()
    }

    fn success_diagnostics(&self, wait_started: Instant) -> LocalAsrDiagnostics {
        LocalAsrDiagnostics {
            pipeline_mode: AsrPipelineMode::ExperimentalNemotronStreaming,
            local_asr_enabled: true,
            local_asr_available: true,
            sidecar_connected: self.snapshot().connected,
            sidecar_start_ms: self.sidecar_start_ms,
            local_asr_session_ms: elapsed_ms(self.started_at),
            local_asr_final_wait_ms: elapsed_ms(wait_started),
            local_asr_error_code: None,
            fallback_to_groq_used: false,
            fallback_reason: None,
        }
    }

    fn failure_diagnostics(
        &self,
        code: LocalAsrErrorCode,
        wait_started: Instant,
    ) -> LocalAsrDiagnostics {
        let safe_code = code.as_safe_str().to_string();
        LocalAsrDiagnostics {
            pipeline_mode: AsrPipelineMode::ExperimentalNemotronStreaming,
            local_asr_enabled: true,
            local_asr_available: self.snapshot().connected,
            sidecar_connected: self.snapshot().connected,
            sidecar_start_ms: self.sidecar_start_ms,
            local_asr_session_ms: elapsed_ms(self.started_at),
            local_asr_final_wait_ms: elapsed_ms(wait_started),
            local_asr_error_code: Some(safe_code.clone()),
            fallback_to_groq_used: true,
            fallback_reason: Some(safe_code),
        }
    }

    async fn cleanup_child(&self) {
        let child = self.child.lock().ok().and_then(|mut slot| slot.take());
        if let Some(mut child) = child {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }
}

#[derive(Default)]
struct SessionSnapshot {
    final_text: Option<String>,
    error_code: Option<LocalAsrErrorCode>,
    connected: bool,
}

#[derive(Default)]
pub struct LocalAsrSidecarManager {
    active: Mutex<Option<LocalAsrSession>>,
    completed: Mutex<Option<LocalAsrOutcome>>,
}

impl LocalAsrSidecarManager {
    pub fn is_enabled(&self) -> bool {
        crate::experiments::nemotron_streaming_stt_enabled()
    }

    pub async fn start_recording_session(&self) -> Option<mpsc::UnboundedSender<SessionCommand>> {
        if !self.is_enabled() {
            self.clear_active();
            return None;
        }

        if let Ok(mut completed) = self.completed.lock() {
            *completed = None;
        }

        if self
            .active
            .lock()
            .map(|active| active.is_some())
            .unwrap_or(true)
        {
            self.store_fallback(LocalAsrErrorCode::Busy, 0, false);
            return None;
        }

        match LocalAsrClient.connect_mock().await {
            Ok(session) => {
                let sender = session.chunk_sender();
                if let Ok(mut active) = self.active.lock() {
                    *active = Some(session);
                    Some(sender)
                } else {
                    None
                }
            }
            Err(error) => {
                self.store_fallback(error.code, 0, false);
                None
            }
        }
    }

    pub async fn finish_recording_session(&self) {
        let session = self.active.lock().ok().and_then(|mut active| active.take());
        if let Some(session) = session {
            let outcome = session.finish().await;
            if let Ok(mut completed) = self.completed.lock() {
                *completed = Some(outcome);
            }
        }
    }

    pub fn cancel_recording_session(&self) {
        let session = self.active.lock().ok().and_then(|mut active| active.take());
        if let Some(session) = session {
            session.cancel();
        }
    }

    pub fn take_completed_outcome(&self) -> Option<LocalAsrOutcome> {
        self.completed
            .lock()
            .ok()
            .and_then(|mut completed| completed.take())
    }

    pub fn disabled_diagnostics(&self) -> LocalAsrDiagnostics {
        LocalAsrDiagnostics::groq_cloud()
    }

    fn clear_active(&self) {
        if let Ok(mut active) = self.active.lock() {
            *active = None;
        }
        if let Ok(mut completed) = self.completed.lock() {
            *completed = None;
        }
    }

    fn store_fallback(&self, code: LocalAsrErrorCode, sidecar_start_ms: u64, connected: bool) {
        let safe_code = code.as_safe_str().to_string();
        let diagnostics = LocalAsrDiagnostics {
            pipeline_mode: AsrPipelineMode::ExperimentalNemotronStreaming,
            local_asr_enabled: true,
            local_asr_available: connected,
            sidecar_connected: connected,
            sidecar_start_ms,
            local_asr_session_ms: 0,
            local_asr_final_wait_ms: 0,
            local_asr_error_code: Some(safe_code.clone()),
            fallback_to_groq_used: true,
            fallback_reason: Some(safe_code),
        };
        if let Ok(mut completed) = self.completed.lock() {
            *completed = Some(LocalAsrOutcome::Fallback(LocalAsrFailure { diagnostics }));
        }
    }
}

#[derive(Default)]
pub struct LocalAsrClient;

impl LocalAsrClient {
    async fn connect_mock(&self) -> Result<LocalAsrSession, LocalAsrError> {
        if std::env::var_os(DISABLE_SIDECAR_ENV).is_some() {
            return Err(LocalAsrError::new(LocalAsrErrorCode::SidecarUnavailable));
        }

        let started = Instant::now();
        let token = generate_session_token()
            .map_err(|_| LocalAsrError::new(LocalAsrErrorCode::Internal))?;
        let scenario = std::env::var(SCENARIO_ENV)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or_default();
        let mut child = spawn_mock_sidecar(&token, scenario)?;
        let port = read_sidecar_port(&mut child).await?;
        let sidecar_start_ms = elapsed_ms(started);
        let connect_url = format!("ws://127.0.0.1:{port}/asr?token={token}");

        let (ws_stream, _) =
            tokio::time::timeout(SIDECAR_START_TIMEOUT, connect_async(connect_url))
                .await
                .map_err(|_| LocalAsrError::new(LocalAsrErrorCode::SidecarStartTimeout))?
                .map_err(|_| LocalAsrError::new(LocalAsrErrorCode::ConnectFailed))?;
        let (mut writer, mut reader) = ws_stream.split();
        let (command_tx, mut command_rx) = mpsc::unbounded_channel::<SessionCommand>();
        let state = Arc::new(Mutex::new(SessionSharedState::default()));
        let reader_state = Arc::clone(&state);
        let watchdog_state = Arc::clone(&state);
        let child = Arc::new(Mutex::new(Some(child)));
        let session_id = new_session_id();

        writer
            .send(Message::Text(
                serde_json::to_string(&ClientMessage::StartSession {
                    sample_rate: 16_000,
                    channels: 1,
                    format: "pcm_s16le".to_string(),
                    language: None,
                    session_id: session_id.clone(),
                })
                .map_err(|_| LocalAsrError::new(LocalAsrErrorCode::Protocol))?,
            ))
            .await
            .map_err(|_| LocalAsrError::new(LocalAsrErrorCode::ConnectFailed))?;

        tokio::spawn(async move {
            let mut sequence = 0_u64;
            while let Some(command) = command_rx.recv().await {
                let message = match command {
                    SessionCommand::Chunk(chunk) => {
                        let mut frame = Vec::with_capacity(16 + chunk.bytes.len());
                        frame.extend_from_slice(&sequence.to_le_bytes());
                        frame.extend_from_slice(&chunk.timestamp_ms.to_le_bytes());
                        frame.extend_from_slice(&chunk.bytes);
                        sequence = sequence.saturating_add(1);
                        Message::Binary(frame)
                    }
                    SessionCommand::End => {
                        let Ok(text) = serde_json::to_string(&ClientMessage::EndOfAudio) else {
                            break;
                        };
                        Message::Text(text)
                    }
                    SessionCommand::Cancel => {
                        let Ok(text) = serde_json::to_string(&ClientMessage::CancelSession) else {
                            break;
                        };
                        let _ = writer.send(Message::Text(text)).await;
                        break;
                    }
                };

                if writer.send(message).await.is_err() {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            while let Some(message) = reader.next().await {
                match message {
                    Ok(Message::Text(text)) => match parse_event(&text) {
                        Ok(event) => apply_event(&reader_state, event),
                        Err(code) => {
                            apply_error(&reader_state, code);
                            break;
                        }
                    },
                    Ok(Message::Close(_)) | Err(_) => {
                        apply_error(&reader_state, LocalAsrErrorCode::Disconnected);
                        break;
                    }
                    _ => {}
                }
            }
        });

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let unhealthy = watchdog_state
                    .lock()
                    .map(|state| {
                        state.final_text.is_none()
                            && state.error_code.is_none()
                            && state
                                .last_heartbeat_at
                                .is_some_and(|last| last.elapsed() > HEARTBEAT_UNHEALTHY_AFTER)
                    })
                    .unwrap_or(false);

                if unhealthy {
                    apply_error(&watchdog_state, LocalAsrErrorCode::HeartbeatTimeout);
                    break;
                }

                let done = watchdog_state
                    .lock()
                    .map(|state| state.final_text.is_some() || state.error_code.is_some())
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        });

        wait_until_connected(&state).await?;

        Ok(LocalAsrSession {
            id: session_id,
            started_at: Instant::now(),
            sidecar_start_ms,
            command_tx,
            state,
            child,
        })
    }
}

fn spawn_mock_sidecar(token: &str, scenario: MockAsrScenario) -> Result<Child, LocalAsrError> {
    let exe = std::env::current_exe()
        .map_err(|_| LocalAsrError::new(LocalAsrErrorCode::SidecarUnavailable))?;
    Command::new(exe)
        .arg(SIDE_CAR_ARG)
        .arg(token)
        .arg(scenario.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| LocalAsrError::new(LocalAsrErrorCode::SidecarUnavailable))
}

async fn read_sidecar_port(child: &mut Child) -> Result<u16, LocalAsrError> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| LocalAsrError::new(LocalAsrErrorCode::SidecarUnavailable))?;
    let mut lines = BufReader::new(stdout).lines();
    let line = tokio::time::timeout(SIDECAR_START_TIMEOUT, lines.next_line())
        .await
        .map_err(|_| LocalAsrError::new(LocalAsrErrorCode::SidecarStartTimeout))?
        .map_err(|_| LocalAsrError::new(LocalAsrErrorCode::SidecarUnavailable))?
        .ok_or_else(|| LocalAsrError::new(LocalAsrErrorCode::SidecarUnavailable))?;
    line.strip_prefix(SIDE_CAR_PORT_PREFIX)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| LocalAsrError::new(LocalAsrErrorCode::SidecarUnavailable))
}

async fn wait_until_connected(state: &Arc<Mutex<SessionSharedState>>) -> Result<(), LocalAsrError> {
    let started = Instant::now();
    loop {
        let snapshot = state
            .lock()
            .map(|state| (state.connected, state.error_code.clone()))
            .unwrap_or((false, Some(LocalAsrErrorCode::Internal)));

        if snapshot.0 {
            return Ok(());
        }
        if let Some(code) = snapshot.1 {
            return Err(LocalAsrError::new(code));
        }
        if started.elapsed() >= SIDECAR_START_TIMEOUT {
            return Err(LocalAsrError::new(LocalAsrErrorCode::StartTimeout));
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn parse_event(text: &str) -> Result<LocalAsrEvent, LocalAsrErrorCode> {
    serde_json::from_str::<LocalAsrEvent>(text).map_err(|_| LocalAsrErrorCode::MalformedEvent)
}

fn apply_event(state: &Arc<Mutex<SessionSharedState>>, event: LocalAsrEvent) {
    if let Ok(mut state) = state.lock() {
        match event {
            LocalAsrEvent::Ready => {
                state.connected = true;
                state.last_heartbeat_at = Some(Instant::now());
            }
            LocalAsrEvent::Heartbeat => {
                state.last_heartbeat_at = Some(Instant::now());
            }
            LocalAsrEvent::PartialTranscript { .. } => {}
            LocalAsrEvent::FinalTranscript { text, stable } => {
                if stable && !text.trim().is_empty() {
                    state.final_text = Some(text);
                } else {
                    state.error_code = Some(LocalAsrErrorCode::MalformedEvent);
                }
            }
            LocalAsrEvent::Error { code } => {
                state.error_code = Some(match code.as_str() {
                    "model_missing" => LocalAsrErrorCode::ModelMissing,
                    "busy" => LocalAsrErrorCode::Busy,
                    _ => LocalAsrErrorCode::Protocol,
                });
            }
        }
    }
}

fn apply_error(state: &Arc<Mutex<SessionSharedState>>, code: LocalAsrErrorCode) {
    if let Ok(mut state) = state.lock() {
        if state.final_text.is_none() && state.error_code.is_none() {
            state.error_code = Some(code);
        }
    }
}

fn generate_session_token() -> Result<String, getrandom::Error> {
    let mut bytes = [0_u8; 24];
    getrandom::getrandom(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_session_id() -> String {
    let counter = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("local-asr-{now}-{counter}")
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

pub fn local_asr_model() -> &'static str {
    LOCAL_ASR_MODEL
}

#[cfg(test)]
mod tests {
    use super::{
        apply_event, configured_pipeline_mode, generate_session_token,
        pipeline_mode_for_nemotron_flag, AsrPipelineMode, ClientMessage, LocalAsrErrorCode,
        LocalAsrEvent, MockAsrScenario, SessionSharedState,
    };
    use std::sync::{Arc, Mutex};

    #[test]
    fn asr_pipeline_mode_defaults_to_groq_cloud() {
        assert_eq!(AsrPipelineMode::default(), AsrPipelineMode::GroqCloud);
        assert_eq!(
            pipeline_mode_for_nemotron_flag(false),
            AsrPipelineMode::GroqCloud
        );
        assert_eq!(
            pipeline_mode_for_nemotron_flag(true),
            AsrPipelineMode::ExperimentalNemotronStreaming
        );
        let _ = configured_pipeline_mode();
    }

    #[test]
    fn protocol_messages_use_stable_snake_case_tags() {
        let start = ClientMessage::StartSession {
            sample_rate: 16_000,
            channels: 1,
            format: "pcm_s16le".to_string(),
            language: None,
            session_id: "session".to_string(),
        };
        let serialized = serde_json::to_string(&start).unwrap();
        assert!(serialized.contains("\"type\":\"start_session\""));
        assert!(serialized.contains("\"sample_rate\":16000"));

        let ready = serde_json::to_string(&LocalAsrEvent::Ready).unwrap();
        assert_eq!(ready, "{\"type\":\"ready\"}");
    }

    #[test]
    fn mock_scenarios_parse_from_safe_names() {
        assert_eq!("success".parse(), Ok(MockAsrScenario::Success));
        assert_eq!("slow-final".parse(), Ok(MockAsrScenario::SlowFinal));
        assert_eq!("model_missing".parse(), Ok(MockAsrScenario::ModelMissing));
        assert!("nemotron".parse::<MockAsrScenario>().is_err());
    }

    #[test]
    fn generated_session_token_is_not_empty() {
        let token = generate_session_token().unwrap();
        assert_eq!(token.len(), 48);
        assert!(token.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn partial_transcripts_are_ignored_in_session_state() {
        let state = Arc::new(Mutex::new(SessionSharedState::default()));
        apply_event(
            &state,
            LocalAsrEvent::PartialTranscript {
                text: "private partial".to_string(),
                stable: false,
            },
        );

        let state = state.lock().unwrap();
        assert!(state.final_text.is_none());
        assert!(state.error_code.is_none());
    }

    #[test]
    fn error_events_map_to_safe_codes() {
        let state = Arc::new(Mutex::new(SessionSharedState::default()));
        apply_event(
            &state,
            LocalAsrEvent::Error {
                code: "model_missing".to_string(),
            },
        );

        assert_eq!(
            state.lock().unwrap().error_code,
            Some(LocalAsrErrorCode::ModelMissing)
        );
    }
}
