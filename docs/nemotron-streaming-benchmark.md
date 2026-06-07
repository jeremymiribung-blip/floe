# Nemotron Streaming Benchmark

This is experimental benchmark infrastructure for evaluating NVIDIA Nemotron 3.5 ASR Streaming 0.6B with Floe-shaped audio. It is not a production rollout.

Floe's stable path remains unchanged: completed recording to Groq file-STT, Groq cleanup, then clipboard/paste. Nemotron is not the default, there is no provider switcher UI, and model weights are not bundled in the normal installer.

## Requirements

- Python 3.10 or newer in a local virtual environment.
- A local PyTorch/NeMo environment suitable for your GPU or CPU.
- The model checkpoint `nvidia/nemotron-3.5-asr-streaming-0.6b`, acquired by NeMo/Hugging Face in your dev environment.
- Fixed benchmark WAV files encoded as 16 kHz mono 16-bit PCM.
- Optional Groq API key in `GROQ_API_KEY` only when running `groq_file_stt` or fallback simulation.

The sidecar handles missing Python packages, missing model, CUDA unavailable, model load failure, malformed requests, timeout, and unavailable model state with safe error codes.

## Setup

```powershell
python -m venv .venv-nemotron
.\.venv-nemotron\Scripts\python -m pip install --upgrade pip
.\.venv-nemotron\Scripts\python -m pip install -r tools\nemotron_sidecar\requirements.txt
.\.venv-nemotron\Scripts\python -m pip install Cython packaging
.\.venv-nemotron\Scripts\python -m pip install "git+https://github.com/NVIDIA/NeMo.git@main#egg=nemo_toolkit[asr]"
```

Install the PyTorch build that matches your CUDA/runtime separately. Do not commit virtual environments, downloaded model files, benchmark WAVs, or generated reports.

## Run The Sidecar

```powershell
.\.venv-nemotron\Scripts\python tools\nemotron_sidecar\floe_nemotron_sidecar.py --host 127.0.0.1 --port 8765 --model-id nvidia/nemotron-3.5-asr-streaming-0.6b
```

Health check:

```powershell
Invoke-WebRequest http://127.0.0.1:8765/health
```

The sidecar exposes:

- `GET /health` for safe readiness, model, runtime, warmup, and memory metadata.
- `WS /asr` for streaming PCM sessions. It accepts 16 kHz mono 16-bit PCM binary frames and emits `ready`, `heartbeat`, `partial_transcript`, `final_transcript`, or safe `error` events.

## Run The Benchmark

Default Nemotron chunk:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin nemotron_benchmark -- --mode nemotron_streaming --wav D:\path\sample.wav --sidecar ws://127.0.0.1:8765/asr --chunk-ms 320 --output nemotron-benchmark.local.json
```

Chunk sweep:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin nemotron_benchmark -- --mode nemotron_with_groq_fallback --wav D:\path\sample.wav --sidecar ws://127.0.0.1:8765/asr --chunk-ms 160,320,560 --output nemotron-sweep.local.json
```

Groq file-STT baseline:

```powershell
$env:GROQ_API_KEY="gsk_..."
cargo run --manifest-path src-tauri/Cargo.toml --bin nemotron_benchmark -- --mode groq_file_stt --wav D:\path\sample.wav --chunk-ms 320 --output groq-baseline.local.json
```

Supported benchmark chunks are `160`, `320`, and `560` ms. The default is `320` ms because Floe does not paste live partials; final transcript stability matters more than ultra-low partial latency.

## Output

Single-chunk runs write one JSON object. Multi-chunk sweeps write `{ "runs": [...] }`.

Each run contains safe metadata:

- `run_id`
- `model_id`
- `runtime`
- `chunk_ms`
- `audio_duration_ms`
- `warmup_ms`
- `local_asr_total_ms`
- `final_wait_ms`
- `realtime_factor`
- `memory_estimate`
- `fallback_used`
- `error_code`
- `transcript_quality_notes` when provided manually

By default the report never includes transcript text, audio data, API keys, auth headers, raw responses, endpoint URLs, clipboard data, or paste data.

For local-only manual quality review, add the explicit dev flag:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin nemotron_benchmark -- --mode nemotron_streaming --wav D:\path\sample.wav --include-transcript-dev --output nemotron-with-text.local.json
```

Do not commit reports generated with `--include-transcript-dev`.

## Privacy Rules

- The benchmark uses fixed WAV files only; it does not require the microphone.
- It does not require clipboard access or paste automation.
- The Python sidecar does not log raw audio.
- The Python sidecar does not log transcripts unless `--include-transcript-log-dev` is explicitly passed.
- The Rust benchmark does not write transcript text unless `--include-transcript-dev` is explicitly passed.
- Audio is never sent for cleanup in this benchmark sidecar path.

## Disable

Stop the Python sidecar process and do not run `nemotron_benchmark`. The normal Floe app path remains Groq-only unless a separate experimental app flag is enabled; this benchmark does not add a UI toggle or make Nemotron the default.

## Known Limitations

- This is a benchmark sidecar, not production integration.
- No ONNX Runtime implementation is included.
- No TensorRT sidecar is included.
- No model downloader is included.
- No model weights are bundled into the regular installer.
- CI uses fake sidecars and fake Groq responses; it does not load the real model.
- The first sidecar implementation is intentionally minimal and should be treated as a local evaluation harness before any production design decisions.

## Productionization Criteria

Consider deeper Nemotron work only if the fixed benchmark set shows:

- German quality near or better than Groq for Floe-style dictation.
- English quality near or better than Groq.
- German-English mixed technical dictation is stable.
- `320` ms chunks have acceptable final transcript stability.
- Realtime factor stays below `1.0` on target hardware.
- Warmup, RAM, and VRAM are acceptable for a desktop utility.
- Repeated runs do not crash or leak memory.
- Punctuation and capitalization are good enough without relying on cleanup to hide ASR defects.
