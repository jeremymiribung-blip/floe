use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use futures_util::{SinkExt, StreamExt};
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const NEMOTRON_MODEL_ID: &str = "nvidia/nemotron-3.5-asr-streaming-0.6b";
const GROQ_STT_MODEL: &str = "whisper-large-v3-turbo";
const DEFAULT_SIDECAR: &str = "ws://127.0.0.1:8765/asr";
const DEFAULT_OUTPUT: &str = "nemotron-benchmark.local.json";
const DEFAULT_GROQ_BASE_URL: &str = "https://api.groq.com";
const CONNECT_TIMEOUT: Duration = Duration::from_millis(1_500);
#[cfg(not(test))]
const FINAL_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(test)]
const FINAL_TIMEOUT: Duration = Duration::from_millis(250);
const GROQ_TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let exit_code = match CliConfig::from_env(env::args().skip(1)) {
        Ok(config) => match run_cli(config).await {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("{error}");
                1
            }
        },
        Err(error) => {
            eprintln!("{error}");
            eprintln!("{}", usage());
            2
        }
    };
    std::process::exit(exit_code);
}

async fn run_cli(config: CliConfig) -> Result<(), String> {
    let wav = WavInput::read(&config.wav_path)?;
    let groq_api_key = env::var("GROQ_API_KEY").ok();
    let mut reports = Vec::new();

    for chunk_ms in &config.chunk_ms_values {
        let report = run_benchmark(
            BenchmarkConfig {
                mode: config.mode,
                sidecar: config.sidecar.clone(),
                chunk_ms: *chunk_ms,
                include_transcript_dev: config.include_transcript_dev,
                transcript_quality_notes: config.transcript_quality_notes.clone(),
                groq_base_url: config.groq_base_url.clone(),
                groq_api_key: groq_api_key.clone(),
            },
            &wav,
        )
        .await;
        reports.push(report);
    }

    let output = if reports.len() == 1 {
        serde_json::to_string_pretty(&reports[0]).map_err(|_| "failed_to_serialize_report")?
    } else {
        serde_json::to_string_pretty(&json!({ "runs": reports }))
            .map_err(|_| "failed_to_serialize_report")?
    };

    if let Some(parent) = config.output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|_| "output_directory_unavailable".to_string())?;
        }
    }
    fs::write(&config.output_path, output).map_err(|_| "output_write_failed".to_string())?;
    println!("wrote {}", config.output_path.display());
    Ok(())
}

#[derive(Debug, Clone)]
struct CliConfig {
    mode: BenchmarkMode,
    wav_path: PathBuf,
    sidecar: String,
    chunk_ms_values: Vec<u16>,
    output_path: PathBuf,
    groq_base_url: String,
    include_transcript_dev: bool,
    transcript_quality_notes: Option<String>,
}

impl CliConfig {
    fn from_env<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut mode = None;
        let mut wav_path = None;
        let mut sidecar = DEFAULT_SIDECAR.to_string();
        let mut chunk_ms_values = vec![320];
        let mut output_path = PathBuf::from(DEFAULT_OUTPUT);
        let mut groq_base_url = DEFAULT_GROQ_BASE_URL.to_string();
        let mut include_transcript_dev = false;
        let mut transcript_quality_notes = None;
        let mut iter = args.into_iter();

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--mode" => mode = Some(parse_mode(next_value(&mut iter, "--mode")?)?),
                "--wav" => wav_path = Some(PathBuf::from(next_value(&mut iter, "--wav")?)),
                "--sidecar" => sidecar = next_value(&mut iter, "--sidecar")?,
                "--chunk-ms" => {
                    chunk_ms_values = parse_chunk_ms_values(&next_value(&mut iter, "--chunk-ms")?)?
                }
                "--output" => output_path = PathBuf::from(next_value(&mut iter, "--output")?),
                "--groq-base-url" => groq_base_url = next_value(&mut iter, "--groq-base-url")?,
                "--transcript-quality-notes" => {
                    transcript_quality_notes =
                        Some(next_value(&mut iter, "--transcript-quality-notes")?)
                }
                "--include-transcript-dev" => include_transcript_dev = true,
                "--help" | "-h" => return Err("help_requested".to_string()),
                unknown => return Err(format!("unknown argument: {unknown}")),
            }
        }

        Ok(Self {
            mode: mode.ok_or_else(|| "missing --mode".to_string())?,
            wav_path: wav_path.ok_or_else(|| "missing --wav".to_string())?,
            sidecar,
            chunk_ms_values,
            output_path,
            groq_base_url,
            include_transcript_dev,
            transcript_quality_notes,
        })
    }
}

fn usage() -> &'static str {
    "usage: nemotron_benchmark --mode <groq_file_stt|nemotron_streaming|nemotron_with_groq_fallback> --wav <path> [--sidecar ws://127.0.0.1:8765/asr] [--chunk-ms 320|160,320,560] [--output report.local.json] [--include-transcript-dev]"
}

fn next_value<I>(iter: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    iter.next()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn parse_mode(value: String) -> Result<BenchmarkMode, String> {
    match value.as_str() {
        "groq_file_stt" => Ok(BenchmarkMode::GroqFileStt),
        "nemotron_streaming" => Ok(BenchmarkMode::NemotronStreaming),
        "nemotron_with_groq_fallback" => Ok(BenchmarkMode::NemotronWithGroqFallback),
        _ => Err(format!("unsupported mode: {value}")),
    }
}

fn parse_chunk_ms_values(value: &str) -> Result<Vec<u16>, String> {
    let mut chunks = Vec::new();
    for part in value.split(',') {
        let chunk = part
            .trim()
            .parse::<u16>()
            .map_err(|_| format!("invalid chunk value: {part}"))?;
        if !matches!(chunk, 160 | 320 | 560) {
            return Err(format!("unsupported chunk_ms: {chunk}"));
        }
        chunks.push(chunk);
    }
    if chunks.is_empty() {
        return Err("chunk list must not be empty".to_string());
    }
    Ok(chunks)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BenchmarkMode {
    GroqFileStt,
    NemotronStreaming,
    NemotronWithGroqFallback,
}

impl BenchmarkMode {
    fn runtime(self) -> &'static str {
        match self {
            Self::GroqFileStt => "groq_file_stt",
            Self::NemotronStreaming => "nemotron_streaming_sidecar",
            Self::NemotronWithGroqFallback => "nemotron_streaming_sidecar_with_groq_fallback",
        }
    }
}

#[derive(Debug, Clone)]
struct BenchmarkConfig {
    mode: BenchmarkMode,
    sidecar: String,
    chunk_ms: u16,
    include_transcript_dev: bool,
    transcript_quality_notes: Option<String>,
    groq_base_url: String,
    groq_api_key: Option<String>,
}

#[derive(Debug, Clone)]
struct WavInput {
    wav_bytes: Vec<u8>,
    pcm_bytes: Vec<u8>,
    audio_duration_ms: u64,
}

impl WavInput {
    fn read(path: &Path) -> Result<Self, String> {
        let wav_bytes = fs::read(path).map_err(|_| "wav_read_failed".to_string())?;
        parse_wav(wav_bytes)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct BenchmarkReport {
    run_id: String,
    model_id: String,
    runtime: String,
    chunk_ms: u16,
    audio_duration_ms: u64,
    warmup_ms: u64,
    local_asr_total_ms: u64,
    final_wait_ms: u64,
    realtime_factor: f64,
    memory_estimate: Option<MemoryEstimate>,
    fallback_used: bool,
    error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transcript_quality_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transcript_text_dev: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
struct MemoryEstimate {
    system_rss_mb: Option<u64>,
    cuda_allocated_mb: Option<u64>,
    cuda_reserved_mb: Option<u64>,
}

async fn run_benchmark(config: BenchmarkConfig, wav: &WavInput) -> BenchmarkReport {
    match config.mode {
        BenchmarkMode::GroqFileStt => run_groq_only(config, wav).await,
        BenchmarkMode::NemotronStreaming => run_nemotron_only(config, wav).await,
        BenchmarkMode::NemotronWithGroqFallback => run_nemotron_with_fallback(config, wav).await,
    }
}

async fn run_groq_only(config: BenchmarkConfig, wav: &WavInput) -> BenchmarkReport {
    let started = Instant::now();
    let groq = run_groq_file_stt(&config, wav).await;
    report_from_outcome(ReportInput {
        model_id: GROQ_STT_MODEL.to_string(),
        runtime: BenchmarkMode::GroqFileStt.runtime().to_string(),
        chunk_ms: config.chunk_ms,
        audio_duration_ms: wav.audio_duration_ms,
        warmup_ms: 0,
        local_asr_total_ms: elapsed_ms(started),
        final_wait_ms: 0,
        memory_estimate: None,
        fallback_used: false,
        error_code: groq.as_ref().err().cloned(),
        transcript: groq.ok(),
        include_transcript_dev: config.include_transcript_dev,
        transcript_quality_notes: config.transcript_quality_notes,
    })
}

async fn run_nemotron_only(config: BenchmarkConfig, wav: &WavInput) -> BenchmarkReport {
    let sidecar = run_sidecar_session(&config, wav).await;
    report_from_sidecar(config, wav, sidecar, false, None)
}

async fn run_nemotron_with_fallback(config: BenchmarkConfig, wav: &WavInput) -> BenchmarkReport {
    let sidecar = run_sidecar_session(&config, wav).await;
    match sidecar {
        Ok(success) => report_from_sidecar(config, wav, Ok(success), false, None),
        Err(sidecar_error) => {
            let groq = run_groq_file_stt(&config, wav).await;
            report_from_sidecar(config, wav, Err(sidecar_error.clone()), true, groq.ok())
        }
    }
}

fn report_from_sidecar(
    config: BenchmarkConfig,
    wav: &WavInput,
    sidecar: Result<SidecarSuccess, String>,
    fallback_used: bool,
    fallback_transcript: Option<String>,
) -> BenchmarkReport {
    match sidecar {
        Ok(success) => report_from_outcome(ReportInput {
            model_id: NEMOTRON_MODEL_ID.to_string(),
            runtime: config.mode.runtime().to_string(),
            chunk_ms: config.chunk_ms,
            audio_duration_ms: wav.audio_duration_ms,
            warmup_ms: success.warmup_ms,
            local_asr_total_ms: success.local_asr_total_ms,
            final_wait_ms: success.final_wait_ms,
            memory_estimate: success.memory_estimate,
            fallback_used,
            error_code: None,
            transcript: Some(success.transcript),
            include_transcript_dev: config.include_transcript_dev,
            transcript_quality_notes: config.transcript_quality_notes,
        }),
        Err(error_code) => report_from_outcome(ReportInput {
            model_id: NEMOTRON_MODEL_ID.to_string(),
            runtime: config.mode.runtime().to_string(),
            chunk_ms: config.chunk_ms,
            audio_duration_ms: wav.audio_duration_ms,
            warmup_ms: 0,
            local_asr_total_ms: 0,
            final_wait_ms: 0,
            memory_estimate: None,
            fallback_used,
            error_code: Some(error_code),
            transcript: fallback_transcript,
            include_transcript_dev: config.include_transcript_dev,
            transcript_quality_notes: config.transcript_quality_notes,
        }),
    }
}

struct ReportInput {
    model_id: String,
    runtime: String,
    chunk_ms: u16,
    audio_duration_ms: u64,
    warmup_ms: u64,
    local_asr_total_ms: u64,
    final_wait_ms: u64,
    memory_estimate: Option<MemoryEstimate>,
    fallback_used: bool,
    error_code: Option<String>,
    transcript: Option<String>,
    include_transcript_dev: bool,
    transcript_quality_notes: Option<String>,
}

fn report_from_outcome(input: ReportInput) -> BenchmarkReport {
    BenchmarkReport {
        run_id: new_run_id(),
        model_id: input.model_id,
        runtime: input.runtime,
        chunk_ms: input.chunk_ms,
        audio_duration_ms: input.audio_duration_ms,
        warmup_ms: input.warmup_ms,
        local_asr_total_ms: input.local_asr_total_ms,
        final_wait_ms: input.final_wait_ms,
        realtime_factor: realtime_factor(input.local_asr_total_ms, input.audio_duration_ms),
        memory_estimate: input.memory_estimate,
        fallback_used: input.fallback_used,
        error_code: input.error_code,
        transcript_quality_notes: input.transcript_quality_notes,
        transcript_text_dev: input
            .include_transcript_dev
            .then_some(input.transcript)
            .flatten(),
    }
}

#[derive(Debug)]
struct SidecarSuccess {
    transcript: String,
    warmup_ms: u64,
    local_asr_total_ms: u64,
    final_wait_ms: u64,
    memory_estimate: Option<MemoryEstimate>,
}

async fn run_sidecar_session(
    config: &BenchmarkConfig,
    wav: &WavInput,
) -> Result<SidecarSuccess, String> {
    let started = Instant::now();
    let connect = tokio::time::timeout(CONNECT_TIMEOUT, connect_async(config.sidecar.as_str()))
        .await
        .map_err(|_| "sidecar_unavailable".to_string())?
        .map_err(|_| "sidecar_unavailable".to_string())?;
    let (ws, _) = connect;
    let (mut writer, mut reader) = ws.split();

    writer
        .send(Message::Text(
            json!({
                "type": "start_session",
                "sample_rate": 16_000,
                "channels": 1,
                "format": "pcm_s16le",
                "language": "auto",
                "chunk_ms": config.chunk_ms,
                "session_id": new_run_id()
            })
            .to_string(),
        ))
        .await
        .map_err(|_| "sidecar_disconnected".to_string())?;

    wait_for_ready(&mut reader).await?;

    let chunk_bytes = bytes_per_chunk(config.chunk_ms);
    let send_started = Instant::now();
    for chunk in wav.pcm_bytes.chunks(chunk_bytes) {
        writer
            .send(Message::Binary(chunk.to_vec()))
            .await
            .map_err(|_| "sidecar_disconnected".to_string())?;
    }

    let final_wait_started = Instant::now();
    writer
        .send(Message::Text(json!({ "type": "end_of_audio" }).to_string()))
        .await
        .map_err(|_| "sidecar_disconnected".to_string())?;

    let final_event = wait_for_final(&mut reader).await?;
    Ok(SidecarSuccess {
        transcript: final_event.text,
        warmup_ms: final_event.warmup_ms.unwrap_or(0),
        local_asr_total_ms: elapsed_ms(started).max(elapsed_ms(send_started)),
        final_wait_ms: elapsed_ms(final_wait_started),
        memory_estimate: final_event.memory_estimate,
    })
}

async fn wait_for_ready<R>(reader: &mut R) -> Result<(), String>
where
    R: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let message = tokio::time::timeout(CONNECT_TIMEOUT, reader.next())
            .await
            .map_err(|_| "start_timeout".to_string())?
            .ok_or_else(|| "sidecar_disconnected".to_string())?
            .map_err(|_| "sidecar_disconnected".to_string())?;
        if let Message::Text(text) = message {
            match parse_sidecar_event(&text)? {
                SidecarEvent::Ready => return Ok(()),
                SidecarEvent::Error { code } => return Err(safe_error_code(&code)),
                _ => {}
            }
        }
    }
}

async fn wait_for_final<R>(reader: &mut R) -> Result<FinalEvent, String>
where
    R: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let message = tokio::time::timeout(FINAL_TIMEOUT, reader.next())
            .await
            .map_err(|_| "timeout".to_string())?
            .ok_or_else(|| "no_final_transcript".to_string())?
            .map_err(|_| "sidecar_disconnected".to_string())?;
        if let Message::Text(text) = message {
            match parse_sidecar_event(&text)? {
                SidecarEvent::Final(final_event) => return Ok(final_event),
                SidecarEvent::Error { code } => return Err(safe_error_code(&code)),
                SidecarEvent::Partial | SidecarEvent::Heartbeat | SidecarEvent::Ready => {}
            }
        }
    }
}

#[derive(Debug)]
enum SidecarEvent {
    Ready,
    Heartbeat,
    Partial,
    Final(FinalEvent),
    Error { code: String },
}

#[derive(Debug)]
struct FinalEvent {
    text: String,
    warmup_ms: Option<u64>,
    memory_estimate: Option<MemoryEstimate>,
}

fn parse_sidecar_event(text: &str) -> Result<SidecarEvent, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "malformed_response".to_string())?;
    let event_type = value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "malformed_response".to_string())?;
    match event_type {
        "ready" => Ok(SidecarEvent::Ready),
        "heartbeat" => Ok(SidecarEvent::Heartbeat),
        "partial_transcript" => Ok(SidecarEvent::Partial),
        "final_transcript" => {
            let text = value
                .get("text")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "malformed_response".to_string())?
                .to_string();
            let memory_estimate = value
                .get("memory_estimate")
                .and_then(|value| serde_json::from_value(value.clone()).ok());
            Ok(SidecarEvent::Final(FinalEvent {
                text,
                warmup_ms: value.get("warmup_ms").and_then(serde_json::Value::as_u64),
                memory_estimate,
            }))
        }
        "error" => {
            let code = value
                .get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("sidecar_error")
                .to_string();
            Ok(SidecarEvent::Error { code })
        }
        _ => Err("malformed_response".to_string()),
    }
}

async fn run_groq_file_stt(config: &BenchmarkConfig, wav: &WavInput) -> Result<String, String> {
    let Some(api_key) = config
        .groq_api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
    else {
        return Err("missing_groq_api_key".to_string());
    };
    let client = reqwest::Client::new();
    let form = multipart::Form::new()
        .text("model", GROQ_STT_MODEL.to_string())
        .text("temperature", "0")
        .part(
            "file",
            multipart::Part::bytes(wav.wav_bytes.clone())
                .file_name("benchmark.wav")
                .mime_str("audio/wav")
                .map_err(|_| "invalid_request".to_string())?,
        );
    let url = format!(
        "{}/openai/v1/audio/transcriptions",
        config.groq_base_url.trim_end_matches('/')
    );
    let response = client
        .post(url)
        .bearer_auth(api_key)
        .timeout(GROQ_TIMEOUT)
        .multipart(form)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                "timeout".to_string()
            } else {
                "api_unreachable".to_string()
            }
        })?;

    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 | 403 => "invalid_groq_api_key",
            429 => "rate_limit",
            400 => "invalid_request",
            415 => "unsupported_audio",
            status if status >= 500 => "server_error",
            _ => "groq_error",
        }
        .to_string());
    }

    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "malformed_response".to_string())?;
    value
        .get("text")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "malformed_response".to_string())
}

fn parse_wav(wav_bytes: Vec<u8>) -> Result<WavInput, String> {
    if wav_bytes.len() < 44
        || wav_bytes.get(0..4) != Some(b"RIFF")
        || wav_bytes.get(8..12) != Some(b"WAVE")
    {
        return Err("unsupported_audio".to_string());
    }

    let mut offset = 12;
    let mut fmt = None;
    let mut data = None;
    while offset + 8 <= wav_bytes.len() {
        let id = &wav_bytes[offset..offset + 4];
        let len = read_u32_le(&wav_bytes, offset + 4)
            .ok_or_else(|| "unsupported_audio".to_string())? as usize;
        let start = offset + 8;
        let end = start
            .checked_add(len)
            .ok_or_else(|| "unsupported_audio".to_string())?;
        if end > wav_bytes.len() {
            return Err("unsupported_audio".to_string());
        }
        if id == b"fmt " {
            fmt = Some((start, len));
        } else if id == b"data" {
            data = Some((start, len));
        }
        offset = end + (len % 2);
    }

    let (fmt_start, fmt_len) = fmt.ok_or_else(|| "unsupported_audio".to_string())?;
    if fmt_len < 16 {
        return Err("unsupported_audio".to_string());
    }
    let audio_format =
        read_u16_le(&wav_bytes, fmt_start).ok_or_else(|| "unsupported_audio".to_string())?;
    let channels =
        read_u16_le(&wav_bytes, fmt_start + 2).ok_or_else(|| "unsupported_audio".to_string())?;
    let sample_rate =
        read_u32_le(&wav_bytes, fmt_start + 4).ok_or_else(|| "unsupported_audio".to_string())?;
    let bits =
        read_u16_le(&wav_bytes, fmt_start + 14).ok_or_else(|| "unsupported_audio".to_string())?;
    if audio_format != 1 || channels != 1 || sample_rate != 16_000 || bits != 16 {
        return Err("unsupported_audio".to_string());
    }

    let (data_start, data_len) = data.ok_or_else(|| "unsupported_audio".to_string())?;
    if data_len == 0 || data_len % 2 != 0 {
        return Err("empty_audio".to_string());
    }
    let pcm_bytes = wav_bytes[data_start..data_start + data_len].to_vec();
    let audio_duration_ms = ((pcm_bytes.len() as u64 / 2) * 1000) / 16_000;
    Ok(WavInput {
        wav_bytes,
        pcm_bytes,
        audio_duration_ms,
    })
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn bytes_per_chunk(chunk_ms: u16) -> usize {
    16_000 * 2 * chunk_ms as usize / 1000
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

fn realtime_factor(total_ms: u64, audio_duration_ms: u64) -> f64 {
    if audio_duration_ms == 0 {
        return 0.0;
    }
    let value = total_ms as f64 / audio_duration_ms as f64;
    (value * 1000.0).round() / 1000.0
}

fn safe_error_code(code: &str) -> String {
    let sanitized = code
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() || sanitized.len() > 64 {
        "sidecar_error".to_string()
    } else {
        sanitized
    }
}

fn new_run_id() -> String {
    let mut bytes = [0_u8; 16];
    if getrandom::getrandom(&mut bytes).is_err() {
        return format!("run-{}", monotonic_id_fallback());
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn monotonic_id_fallback() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        thread,
    };
    use tokio_tungstenite::accept_async;

    #[test]
    fn chunk_parser_accepts_only_benchmark_chunks() {
        assert_eq!(
            parse_chunk_ms_values("160,320,560").unwrap(),
            vec![160, 320, 560]
        );
        assert!(parse_chunk_ms_values("80").is_err());
        assert!(parse_chunk_ms_values("").is_err());
    }

    #[test]
    fn benchmark_json_omits_transcript_by_default() {
        let report = report_from_outcome(ReportInput {
            model_id: NEMOTRON_MODEL_ID.to_string(),
            runtime: "nemotron_streaming_sidecar".to_string(),
            chunk_ms: 320,
            audio_duration_ms: 1000,
            warmup_ms: 1,
            local_asr_total_ms: 500,
            final_wait_ms: 20,
            memory_estimate: None,
            fallback_used: false,
            error_code: None,
            transcript: Some("private transcript gsk_secret".to_string()),
            include_transcript_dev: false,
            transcript_quality_notes: None,
        });

        let json = serde_json::to_string(&report).unwrap();

        assert!(!json.contains("private transcript"));
        assert!(!json.contains("gsk_secret"));
        assert!(!json.contains("transcript_text_dev"));
    }

    #[test]
    fn dev_flag_is_required_for_transcript_output() {
        let report = report_from_outcome(ReportInput {
            model_id: NEMOTRON_MODEL_ID.to_string(),
            runtime: "nemotron_streaming_sidecar".to_string(),
            chunk_ms: 320,
            audio_duration_ms: 1000,
            warmup_ms: 1,
            local_asr_total_ms: 500,
            final_wait_ms: 20,
            memory_estimate: None,
            fallback_used: false,
            error_code: None,
            transcript: Some("manual local transcript".to_string()),
            include_transcript_dev: true,
            transcript_quality_notes: None,
        });

        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("transcript_text_dev"));
        assert!(json.contains("manual local transcript"));
    }

    #[tokio::test]
    async fn sidecar_unavailable_returns_safe_error() {
        let wav = fixture_wav();
        let config = BenchmarkConfig {
            mode: BenchmarkMode::NemotronStreaming,
            sidecar: "ws://127.0.0.1:9/asr".to_string(),
            chunk_ms: 320,
            include_transcript_dev: false,
            transcript_quality_notes: None,
            groq_base_url: DEFAULT_GROQ_BASE_URL.to_string(),
            groq_api_key: None,
        };

        let report = run_benchmark(config, &wav).await;

        assert_eq!(report.error_code.as_deref(), Some("sidecar_unavailable"));
        assert!(!report.fallback_used);
        assert!(report.transcript_text_dev.is_none());
    }

    #[tokio::test]
    async fn fake_nemotron_sidecar_success() {
        let server = FakeSidecar::start(FakeSidecarScenario::Success).await;
        let wav = fixture_wav();
        let config = BenchmarkConfig {
            mode: BenchmarkMode::NemotronStreaming,
            sidecar: server.url(),
            chunk_ms: 320,
            include_transcript_dev: false,
            transcript_quality_notes: None,
            groq_base_url: DEFAULT_GROQ_BASE_URL.to_string(),
            groq_api_key: None,
        };

        let report = run_benchmark(config, &wav).await;

        assert_eq!(report.error_code, None);
        assert!(!report.fallback_used);
        assert_eq!(report.memory_estimate.unwrap().system_rss_mb, Some(123));
        assert!(report.transcript_text_dev.is_none());
    }

    #[tokio::test]
    async fn fake_sidecar_timeout_is_safe() {
        let server = FakeSidecar::start(FakeSidecarScenario::Timeout).await;
        let wav = fixture_wav();
        let config = BenchmarkConfig {
            mode: BenchmarkMode::NemotronStreaming,
            sidecar: server.url(),
            chunk_ms: 320,
            include_transcript_dev: false,
            transcript_quality_notes: None,
            groq_base_url: DEFAULT_GROQ_BASE_URL.to_string(),
            groq_api_key: None,
        };

        let report = run_benchmark(config, &wav).await;

        assert_eq!(report.error_code.as_deref(), Some("timeout"));
        assert!(!report.fallback_used);
    }

    #[tokio::test]
    async fn fake_sidecar_malformed_output_is_safe() {
        let server = FakeSidecar::start(FakeSidecarScenario::Malformed).await;
        let wav = fixture_wav();
        let config = BenchmarkConfig {
            mode: BenchmarkMode::NemotronStreaming,
            sidecar: server.url(),
            chunk_ms: 320,
            include_transcript_dev: false,
            transcript_quality_notes: None,
            groq_base_url: DEFAULT_GROQ_BASE_URL.to_string(),
            groq_api_key: None,
        };

        let report = run_benchmark(config, &wav).await;

        assert_eq!(report.error_code.as_deref(), Some("malformed_response"));
    }

    #[tokio::test]
    async fn fallback_to_groq_path_if_enabled() {
        let sidecar = FakeSidecar::start(FakeSidecarScenario::Error).await;
        let groq = FakeGroq::start(r#"{"text":"groq fallback text"}"#);
        let wav = fixture_wav();
        let config = BenchmarkConfig {
            mode: BenchmarkMode::NemotronWithGroqFallback,
            sidecar: sidecar.url(),
            chunk_ms: 320,
            include_transcript_dev: false,
            transcript_quality_notes: None,
            groq_base_url: groq.base_url(),
            groq_api_key: Some("gsk_test_key".to_string()),
        };

        let report = run_benchmark(config, &wav).await;

        assert!(report.fallback_used);
        assert_eq!(report.error_code.as_deref(), Some("model_missing"));
        assert!(report.transcript_text_dev.is_none());
        assert_eq!(groq.request_count(), 1);
    }

    #[test]
    fn wav_parser_requires_16khz_mono_pcm16() {
        let wav = fixture_wav();
        assert_eq!(wav.audio_duration_ms, 1000);
        assert_eq!(wav.pcm_bytes.len(), 32_000);

        let mut invalid = wav.wav_bytes.clone();
        invalid[22] = 2;
        assert_eq!(parse_wav(invalid).unwrap_err(), "unsupported_audio");
    }

    struct FakeSidecar {
        addr: String,
    }

    enum FakeSidecarScenario {
        Success,
        Timeout,
        Malformed,
        Error,
    }

    impl FakeSidecar {
        async fn start(scenario: FakeSidecarScenario) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let Ok(ws) = accept_async(stream).await else {
                    return;
                };
                let (mut writer, mut reader) = ws.split();
                match scenario {
                    FakeSidecarScenario::Timeout => {
                        let _ = writer
                            .send(Message::Text(r#"{"type":"ready"}"#.to_string()))
                            .await;
                        tokio::time::sleep(Duration::from_secs(31)).await;
                    }
                    FakeSidecarScenario::Malformed => {
                        let _ = writer.send(Message::Text("{".to_string())).await;
                    }
                    FakeSidecarScenario::Error => {
                        let _ = writer
                            .send(Message::Text(r#"{"type":"ready"}"#.to_string()))
                            .await;
                        while let Some(Ok(message)) = reader.next().await {
                            if let Message::Text(text) = message {
                                if text.contains("end_of_audio") {
                                    let _ = writer
                                        .send(Message::Text(
                                            r#"{"type":"error","code":"model_missing"}"#
                                                .to_string(),
                                        ))
                                        .await;
                                    break;
                                }
                            }
                        }
                    }
                    FakeSidecarScenario::Success => {
                        let _ = writer
                            .send(Message::Text(r#"{"type":"ready"}"#.to_string()))
                            .await;
                        while let Some(Ok(message)) = reader.next().await {
                            if let Message::Text(text) = message {
                                if text.contains("end_of_audio") {
                                    let _ = writer
                                        .send(Message::Text(
                                            json!({
                                                "type": "partial_transcript",
                                                "text": "private partial",
                                                "stable": false
                                            })
                                            .to_string(),
                                        ))
                                        .await;
                                    let _ = writer
                                        .send(Message::Text(
                                            json!({
                                                "type": "final_transcript",
                                                "text": "private final transcript",
                                                "stable": true,
                                                "warmup_ms": 7,
                                                "memory_estimate": {
                                                    "system_rss_mb": 123,
                                                    "cuda_allocated_mb": null,
                                                    "cuda_reserved_mb": null
                                                }
                                            })
                                            .to_string(),
                                        ))
                                        .await;
                                    break;
                                }
                            }
                        }
                    }
                }
            });
            Self {
                addr: format!("ws://{addr}/asr"),
            }
        }

        fn url(&self) -> String {
            self.addr.clone()
        }
    }

    struct FakeGroq {
        addr: String,
        request_count: Arc<AtomicUsize>,
    }

    impl FakeGroq {
        fn start(body: &'static str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let request_count = Arc::new(AtomicUsize::new(0));
            let thread_count = Arc::clone(&request_count);
            thread::spawn(move || {
                if let Ok((mut stream, _)) = listener.accept() {
                    let _ = read_request(&mut stream);
                    thread_count.fetch_add(1, Ordering::SeqCst);
                    write_response(&mut stream, 200, body);
                }
            });
            Self {
                addr: format!("http://{addr}"),
                request_count,
            }
        }

        fn base_url(&self) -> String {
            self.addr.clone()
        }

        fn request_count(&self) -> usize {
            self.request_count.load(Ordering::SeqCst)
        }
    }

    fn fixture_wav() -> WavInput {
        parse_wav(silence_wav(16_000)).unwrap()
    }

    fn silence_wav(samples: usize) -> Vec<u8> {
        let data_len = samples * 2;
        let mut wav = Vec::with_capacity(44 + data_len);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&32_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data_len as u32).to_le_bytes());
        wav.resize(44 + data_len, 0);
        wav
    }

    fn read_request(stream: &mut TcpStream) -> Vec<u8> {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buffer = [0_u8; 4096];
        let mut request = Vec::new();
        loop {
            let read = stream.read(&mut buffer).unwrap_or(0);
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        request
    }

    fn write_response(stream: &mut TcpStream, status: u16, body: &str) {
        let raw = format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(raw.as_bytes());
        let _ = stream.flush();
    }
}
