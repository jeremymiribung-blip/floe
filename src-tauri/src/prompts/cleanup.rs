pub const CLEANUP_SYSTEM_PROMPT: &str = "You are a transcript cleanup engine for a push-to-talk dictation app.\n\nYour job:\n- Correct capitalization.\n- Correct punctuation.\n- Correct grammar only when the meaning stays the same.\n- Fix obvious speech-to-text errors when the intended word is clear.\n- Fix obvious misrecognized technical terms when they match the vocabulary below.\n- Preserve the original language.\n- Preserve the user's meaning.\n- Preserve the user's tone.\n- Do not summarize.\n- Do not rewrite stylistically.\n- Do not add information.\n- Do not remove information.\n- Return only the cleaned transcript text.\n\nTechnical vocabulary:\nFloe, Groq, Tauri, TypeScript, Rust, GitHub, Pull Request, Branch, Draft, Clipboard, Hotkey, Single Instance Lock, Whisper Large V3 Turbo, Llama 3.1 8B Instant, Llama 3.3 70B Versatile, OpenCode, MiniMax, Codex, Parakeet, Cleanup, mergen, nicht mergen.\n\nOutput rules:\n- Do not use JSON.\n- Do not use YAML.\n- Do not use Markdown.\n- Do not use quotes.\n- Do not use labels.\n- Do not explain your changes.";

#[cfg(test)]
mod tests {
    use super::CLEANUP_SYSTEM_PROMPT;

    #[test]
    fn prompt_includes_technical_vocabulary() {
        for term in [
            "Floe",
            "Groq",
            "Tauri",
            "TypeScript",
            "Rust",
            "GitHub",
            "Pull Request",
            "Branch",
            "Draft",
            "Clipboard",
            "Hotkey",
            "Single Instance Lock",
            "Whisper Large V3 Turbo",
            "Llama 3.1 8B Instant",
            "Llama 3.3 70B Versatile",
            "OpenCode",
            "MiniMax",
            "Codex",
            "Parakeet",
            "Cleanup",
            "mergen",
            "nicht mergen",
        ] {
            assert!(
                CLEANUP_SYSTEM_PROMPT.contains(term),
                "cleanup prompt must list technical term: {term}"
            );
        }
    }

    #[test]
    fn prompt_forbids_stylistic_rewriting_and_explanations() {
        assert!(CLEANUP_SYSTEM_PROMPT.contains("Do not rewrite stylistically."));
        assert!(CLEANUP_SYSTEM_PROMPT.contains("Do not summarize."));
        assert!(CLEANUP_SYSTEM_PROMPT.contains("Do not explain your changes."));
        assert!(CLEANUP_SYSTEM_PROMPT.contains("Do not add information."));
        assert!(CLEANUP_SYSTEM_PROMPT.contains("Do not remove information."));
    }
}
