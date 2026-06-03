# Floe Agent Notes

- Respect the STT rule: one Groq STT request after recording stops.
- Do not add streaming, chunking, transcript merging, or realtime partials.
- V1/Fast mode uses local cleanup only.
- AI cleanup is optional, disabled by default, and only runs in Clean mode.
- Audio is never sent to Cerebras.
- Only transcript text may be sent to Cerebras when the user explicitly enables Clean cleanup.
- Keep audio in memory only by default.
- Keep secret and non-secret settings separate.
- Do not log raw transcripts, raw audio, full API keys, or auth headers.
- Prefer small modules and focused tests.
