# Floe Agent Notes

- Respect the V1 rule: one Groq STT request after recording stops.
- Do not add streaming, chunking, transcript merging, realtime partials, or LLM cleanup.
- Keep audio in memory only by default.
- Keep secret and non-secret settings separate.
- Do not log raw transcripts, raw audio, full API keys, or auth headers.
- Prefer small modules and focused tests.
