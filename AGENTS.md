# Floe Agent Notes

- Respect the STT rule: one Groq STT request after recording stops.
- Do not add streaming, chunking, transcript merging, or realtime partials.
- Floe uses Groq for STT.
- Floe uses Groq for transcript cleanup; there is no provider switching, no cleanup modes, and no behavior settings.
- The same Groq API key handles both STT and cleanup; it is stored under `groq-api-key` in the OS keychain.
- Audio is never sent for cleanup. Only transcript text is sent for cleanup.
- If cleanup fails, Floe falls back to pasting the raw Groq transcript and surfaces a `Cleanup failed` warning.
- Keep audio in memory only by default.
- Keep secret and non-secret settings separate.
- Keep the configurable push-to-talk hotkey in non-secret settings; default to Alt+Space (shown as Option + Space) on macOS and Control+Space (shown as Ctrl + Space) on Windows/Linux.
- Start at login is optional, uses OS autostart state, and should launch Floe hidden with the tray and hotkey ready.
- Do not access the microphone, start recording, call Groq, paste text, or show prompts during background startup.
- Do not log raw transcripts, raw audio, full API keys, or auth headers.
- Prefer small modules and focused tests.
- Floe runs as a single app instance via the Tauri 2 single-instance plugin; secondary launches show/focus the existing main window and never reinitialize the tray, hotkey, audio manager, recording, or paste.
- The main window is gated by a `setupState` (`setup_groq`, `setup_hotkey`, `ready`) derived from the live Groq key and hotkey status. Onboarding shows when the key is missing or the hotkey is not registered; it does not call Groq, the microphone, recording, or paste. If the key is cleared or the hotkey becomes invalid later, the app returns to the matching setup step. The overview never shows cleanup modes, Behavior, or provider labels.
