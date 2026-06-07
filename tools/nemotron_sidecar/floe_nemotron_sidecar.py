#!/usr/bin/env python3
"""Experimental Floe Nemotron/NeMo ASR sidecar.

This file is dev-only benchmark infrastructure. It is not imported by the
Tauri app, does not download or bundle model weights for the installer, and
does not log raw audio or transcript text by default.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import platform
import tempfile
import time
import wave
from dataclasses import dataclass
from http import HTTPStatus
from typing import Any

try:
    import websockets
    from websockets.server import WebSocketServerProtocol
except Exception as exc:  # pragma: no cover - exercised manually.
    print(
        json.dumps(
            {
                "level": "error",
                "code": "missing_package",
                "package": "websockets",
                "message": "Install websockets before running the benchmark sidecar.",
            }
        )
    )
    raise SystemExit(2) from exc


DEFAULT_MODEL_ID = "nvidia/nemotron-3.5-asr-streaming-0.6b"
SAFE_ERROR_CODES = {
    "missing_package",
    "missing_model",
    "cuda_unavailable",
    "model_load_failed",
    "model_unavailable",
    "malformed_request",
    "unsupported_audio",
    "timeout",
    "internal",
}


@dataclass
class RuntimeState:
    model_id: str
    device: str
    target_lang: str
    include_transcript_log_dev: bool
    ready: bool = False
    runtime: str = "python_nemo"
    error_code: str | None = None
    warmup_ms: int = 0
    model: Any | None = None
    torch: Any | None = None

    def load(self) -> None:
        started = time.perf_counter()
        try:
            import torch  # type: ignore
            import nemo.collections.asr as nemo_asr  # type: ignore
        except Exception:
            self.error_code = "missing_package"
            return

        self.torch = torch
        selected_device = self._selected_device(torch)
        if selected_device == "cuda_unavailable":
            self.error_code = "cuda_unavailable"
            return

        try:
            model = nemo_asr.models.ASRModel.from_pretrained(model_name=self.model_id)
            if hasattr(model, "set_inference_prompt"):
                model.set_inference_prompt(self.target_lang)
            if hasattr(model, "eval"):
                model.eval()
            if hasattr(model, "to"):
                model = model.to(selected_device)
            self.model = model
            self.device = selected_device
            self.ready = True
            self.error_code = None
            self.warmup_ms = elapsed_ms(started)
        except FileNotFoundError:
            self.error_code = "missing_model"
        except Exception:
            self.error_code = "model_load_failed"

    def _selected_device(self, torch: Any) -> str:
        if self.device == "cpu":
            return "cpu"
        if self.device == "cuda":
            return "cuda" if torch.cuda.is_available() else "cuda_unavailable"
        return "cuda" if torch.cuda.is_available() else "cpu"

    async def transcribe_pcm(self, pcm: bytes) -> str:
        if not self.ready or self.model is None:
            raise SafeSidecarError("model_unavailable")
        if not pcm or len(pcm) % 2 != 0:
            raise SafeSidecarError("unsupported_audio")

        loop = asyncio.get_running_loop()
        return await loop.run_in_executor(None, self._transcribe_pcm_blocking, pcm)

    def _transcribe_pcm_blocking(self, pcm: bytes) -> str:
        with tempfile.NamedTemporaryFile(suffix=".wav", delete=True) as tmp:
            with wave.open(tmp.name, "wb") as wav:
                wav.setnchannels(1)
                wav.setsampwidth(2)
                wav.setframerate(16_000)
                wav.writeframes(pcm)

            result = self.model.transcribe([tmp.name])
            text = extract_text(result)
            if self.include_transcript_log_dev:
                print(json.dumps({"level": "debug", "event": "transcript_text_dev", "text": text}))
            return text

    def health(self) -> dict[str, Any]:
        return {
            "status": "ok" if self.ready else "degraded",
            "ready": self.ready,
            "model_id": self.model_id,
            "runtime": self.runtime,
            "device": self.device,
            "target_lang": self.target_lang,
            "warmup_ms": self.warmup_ms,
            "error_code": self.error_code,
            "python": platform.python_version(),
            "memory_estimate": memory_estimate(self.torch),
        }


class SafeSidecarError(Exception):
    def __init__(self, code: str) -> None:
        super().__init__(code if code in SAFE_ERROR_CODES else "internal")
        self.code = code if code in SAFE_ERROR_CODES else "internal"


async def handle_asr(websocket: WebSocketServerProtocol, runtime: RuntimeState) -> None:
    if not runtime.ready:
        await websocket.send(event("error", code=runtime.error_code or "model_unavailable"))
        return

    pcm = bytearray()
    started = time.perf_counter()
    session_started = False
    await websocket.send(event("ready"))

    try:
        async for message in websocket:
            if isinstance(message, bytes):
                if not session_started:
                    await websocket.send(event("error", code="malformed_request"))
                    return
                pcm.extend(message)
                await websocket.send(event("heartbeat"))
                continue

            try:
                payload = json.loads(message)
            except json.JSONDecodeError:
                await websocket.send(event("error", code="malformed_request"))
                return

            message_type = payload.get("type")
            if message_type == "start_session":
                if not valid_start_session(payload):
                    await websocket.send(event("error", code="unsupported_audio"))
                    return
                session_started = True
            elif message_type == "end_of_audio":
                final_started = time.perf_counter()
                try:
                    text = await asyncio.wait_for(runtime.transcribe_pcm(bytes(pcm)), timeout=120)
                except asyncio.TimeoutError:
                    await websocket.send(event("error", code="timeout"))
                    return
                except SafeSidecarError as exc:
                    await websocket.send(event("error", code=exc.code))
                    return
                except Exception:
                    await websocket.send(event("error", code="internal"))
                    return

                await websocket.send(
                    event(
                        "partial_transcript",
                        text=text,
                        stable=False,
                    )
                )
                await websocket.send(
                    event(
                        "final_transcript",
                        text=text,
                        stable=True,
                        warmup_ms=runtime.warmup_ms,
                        local_asr_total_ms=elapsed_ms(started),
                        final_wait_ms=elapsed_ms(final_started),
                        memory_estimate=memory_estimate(runtime.torch),
                    )
                )
                return
            elif message_type == "cancel_session":
                return
            else:
                await websocket.send(event("error", code="malformed_request"))
                return
    except Exception:
        return


def valid_start_session(payload: dict[str, Any]) -> bool:
    return (
        payload.get("sample_rate") == 16_000
        and payload.get("channels") == 1
        and payload.get("format") == "pcm_s16le"
    )


def event(kind: str, **fields: Any) -> str:
    return json.dumps({"type": kind, **fields}, separators=(",", ":"))


def extract_text(result: Any) -> str:
    if isinstance(result, str):
        return result.strip()
    if isinstance(result, list) and result:
        return extract_text(result[0])
    if isinstance(result, tuple) and result:
        return extract_text(result[0])
    text = getattr(result, "text", None)
    if isinstance(text, str):
        return text.strip()
    return str(result).strip()


def memory_estimate(torch_module: Any | None) -> dict[str, int | None]:
    rss_mb = None
    try:
        import os
        import psutil  # type: ignore

        rss_mb = int(psutil.Process(os.getpid()).memory_info().rss / (1024 * 1024))
    except Exception:
        rss_mb = None

    cuda_allocated_mb = None
    cuda_reserved_mb = None
    try:
        if torch_module is not None and torch_module.cuda.is_available():
            cuda_allocated_mb = int(torch_module.cuda.memory_allocated() / (1024 * 1024))
            cuda_reserved_mb = int(torch_module.cuda.memory_reserved() / (1024 * 1024))
    except Exception:
        cuda_allocated_mb = None
        cuda_reserved_mb = None

    return {
        "system_rss_mb": rss_mb,
        "cuda_allocated_mb": cuda_allocated_mb,
        "cuda_reserved_mb": cuda_reserved_mb,
    }


def elapsed_ms(started: float) -> int:
    return max(0, round((time.perf_counter() - started) * 1000))


def health_response(runtime: RuntimeState) -> tuple[HTTPStatus, list[tuple[str, str]], bytes]:
    body = json.dumps(runtime.health(), separators=(",", ":")).encode("utf-8")
    return (
        HTTPStatus.OK,
        [("Content-Type", "application/json"), ("Content-Length", str(len(body)))],
        body,
    )


def not_found_response() -> tuple[HTTPStatus, list[tuple[str, str]], bytes]:
    body = b'{"error":"not_found"}'
    return (
        HTTPStatus.NOT_FOUND,
        [("Content-Type", "application/json"), ("Content-Length", str(len(body)))],
        body,
    )


async def main() -> None:
    parser = argparse.ArgumentParser(description="Floe experimental Nemotron benchmark sidecar")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--model-id", default=DEFAULT_MODEL_ID)
    parser.add_argument("--device", choices=["auto", "cuda", "cpu"], default="auto")
    parser.add_argument("--target-lang", default="auto")
    parser.add_argument("--include-transcript-log-dev", action="store_true")
    args = parser.parse_args()

    runtime = RuntimeState(
        model_id=args.model_id,
        device=args.device,
        target_lang=args.target_lang,
        include_transcript_log_dev=args.include_transcript_log_dev,
    )
    runtime.load()

    async def process_request(path: str, _headers: Any) -> Any:
        if path == "/health":
            return health_response(runtime)
        if path != "/asr":
            return not_found_response()
        return None

    async def handler(websocket: WebSocketServerProtocol, path: str) -> None:
        if path != "/asr":
            await websocket.close(code=1008, reason="unsupported_path")
            return
        await handle_asr(websocket, runtime)

    print(
        json.dumps(
            {
                "level": "info",
                "event": "sidecar_started",
                "host": args.host,
                "port": args.port,
                "ready": runtime.ready,
                "error_code": runtime.error_code,
            },
            separators=(",", ":"),
        )
    )
    async with websockets.serve(
        handler,
        args.host,
        args.port,
        process_request=process_request,
        max_size=None,
    ):
        await asyncio.Future()


if __name__ == "__main__":
    asyncio.run(main())
