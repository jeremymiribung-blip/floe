# Floe Agent Notes

- Respect the STT rule: one Groq STT request after recording stops.
- Do not add streaming, chunking, transcript merging, or realtime partials.
- Floe uses Groq for STT.
- Floe uses Cerebras for transcript cleanup.
- Cerebras cleanup is part of the standard flow; there are no cleanup modes.
- Audio is never sent to Cerebras. Only transcript text is sent to Cerebras.
- If Cerebras cleanup fails, Floe falls back to pasting the raw Groq transcript and surfaces a `Cleanup failed` warning.
- Keep audio in memory only by default.
- Keep secret and non-secret settings separate.
- Keep the configurable push-to-talk hotkey in non-secret settings; default to CommandOrControl+Shift+Space on macOS and Control+Shift+Space on Windows/Linux.
- Start at login is optional, uses OS autostart state, and should launch Floe hidden with the tray and hotkey ready.
- Do not access the microphone, start recording, call Groq, call Cerebras, paste text, or show prompts during background startup.
- Do not log raw transcripts, raw audio, full API keys, or auth headers.
- Prefer small modules and focused tests.
