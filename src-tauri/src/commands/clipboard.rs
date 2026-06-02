use std::time::Duration;

use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

const CLIPBOARD_SETTLE_DELAY: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardError {
    pub code: ClipboardErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ClipboardErrorCode {
    ClipboardUnavailable,
    PasteUnavailable,
}

trait TextClipboard {
    fn write_text(&self, text: &str) -> Result<(), ClipboardError>;
}

trait PasteSimulator {
    fn paste_shortcut(&self, shortcut: PasteShortcut) -> Result<(), ClipboardError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PasteShortcut {
    modifier: PasteModifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasteModifier {
    Control,
    Meta,
}

struct TauriTextClipboard {
    app: AppHandle,
}

impl TextClipboard for TauriTextClipboard {
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        self.app
            .clipboard()
            .write_text(text.to_string())
            .map_err(|_| clipboard_unavailable_error())
    }
}

struct EnigoPasteSimulator;

impl PasteSimulator for EnigoPasteSimulator {
    fn paste_shortcut(&self, shortcut: PasteShortcut) -> Result<(), ClipboardError> {
        let mut enigo = Enigo::new(&Settings::default()).map_err(|_| paste_unavailable_error())?;
        let modifier = enigo_modifier_key(shortcut.modifier);

        enigo
            .key(modifier, Press)
            .map_err(|_| paste_unavailable_error())?;
        let paste_result = enigo
            .key(Key::Unicode('v'), Click)
            .map_err(|_| paste_unavailable_error());
        let release_result = enigo
            .key(modifier, Release)
            .map_err(|_| paste_unavailable_error());

        paste_result?;
        release_result
    }
}

#[tauri::command]
pub fn copy_text_to_clipboard(app: AppHandle, text: String) -> Result<(), ClipboardError> {
    let clipboard = TauriTextClipboard { app };

    copy_text_to_clipboard_with(&clipboard, &text)
}

#[tauri::command]
pub fn paste_text(app: AppHandle, text: String) -> Result<(), ClipboardError> {
    let clipboard = TauriTextClipboard { app };
    let paste_simulator = EnigoPasteSimulator;

    paste_text_with(&clipboard, &paste_simulator, &text, std::thread::sleep)
}

fn copy_text_to_clipboard_with(
    clipboard: &impl TextClipboard,
    text: &str,
) -> Result<(), ClipboardError> {
    clipboard.write_text(text)
}

fn paste_text_with(
    clipboard: &impl TextClipboard,
    paste_simulator: &impl PasteSimulator,
    text: &str,
    delay: impl FnOnce(Duration),
) -> Result<(), ClipboardError> {
    clipboard.write_text(text)?;
    delay(CLIPBOARD_SETTLE_DELAY);
    paste_simulator.paste_shortcut(current_paste_shortcut())
}

fn current_paste_shortcut() -> PasteShortcut {
    paste_shortcut_for_target_os(std::env::consts::OS)
}

fn paste_shortcut_for_target_os(os: &str) -> PasteShortcut {
    let modifier = if os == "macos" {
        PasteModifier::Meta
    } else {
        PasteModifier::Control
    };

    PasteShortcut { modifier }
}

fn enigo_modifier_key(modifier: PasteModifier) -> Key {
    match modifier {
        PasteModifier::Control => Key::Control,
        PasteModifier::Meta => Key::Meta,
    }
}

fn clipboard_unavailable_error() -> ClipboardError {
    clipboard_error(
        ClipboardErrorCode::ClipboardUnavailable,
        "Floe could not write to the clipboard.",
    )
}

fn paste_unavailable_error() -> ClipboardError {
    clipboard_error(
        ClipboardErrorCode::PasteUnavailable,
        "Transcript copied to clipboard, but Floe could not send the paste shortcut. Paste manually with Command+V or Control+V.",
    )
}

fn clipboard_error(code: ClipboardErrorCode, message: &'static str) -> ClipboardError {
    ClipboardError {
        code,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::{
        clipboard_unavailable_error, copy_text_to_clipboard_with, paste_shortcut_for_target_os,
        paste_text_with, paste_unavailable_error, ClipboardError, ClipboardErrorCode,
        PasteModifier, PasteShortcut, PasteSimulator, TextClipboard, CLIPBOARD_SETTLE_DELAY,
    };

    #[derive(Default)]
    struct FakeClipboard {
        text: RefCell<Option<String>>,
        fail_write: bool,
    }

    impl TextClipboard for FakeClipboard {
        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            if self.fail_write {
                return Err(clipboard_unavailable_error());
            }

            *self.text.borrow_mut() = Some(text.to_string());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakePasteSimulator {
        shortcuts: RefCell<Vec<PasteShortcut>>,
        fail_paste: bool,
    }

    impl PasteSimulator for FakePasteSimulator {
        fn paste_shortcut(&self, shortcut: PasteShortcut) -> Result<(), ClipboardError> {
            self.shortcuts.borrow_mut().push(shortcut);

            if self.fail_paste {
                return Err(paste_unavailable_error());
            }

            Ok(())
        }
    }

    #[test]
    fn copy_writes_exact_text() {
        let clipboard = FakeClipboard::default();

        copy_text_to_clipboard_with(&clipboard, "hello from Floe").unwrap();

        assert_eq!(clipboard.text.borrow().as_deref(), Some("hello from Floe"));
    }

    #[test]
    fn paste_writes_clipboard_before_simulating_shortcut() {
        let clipboard = FakeClipboard::default();
        let paste_simulator = FakePasteSimulator::default();
        let mut observed_delay = None;

        paste_text_with(&clipboard, &paste_simulator, "paste me", |delay| {
            observed_delay = Some(delay)
        })
        .unwrap();

        assert_eq!(clipboard.text.borrow().as_deref(), Some("paste me"));
        assert_eq!(observed_delay, Some(CLIPBOARD_SETTLE_DELAY));
        assert_eq!(paste_simulator.shortcuts.borrow().len(), 1);
    }

    #[test]
    fn paste_failure_leaves_text_in_clipboard() {
        let clipboard = FakeClipboard::default();
        let paste_simulator = FakePasteSimulator {
            fail_paste: true,
            ..Default::default()
        };

        let error = paste_text_with(&clipboard, &paste_simulator, "still copied", |_| {})
            .expect_err("paste failure should return an error");

        assert_eq!(error.code, ClipboardErrorCode::PasteUnavailable);
        assert_eq!(clipboard.text.borrow().as_deref(), Some("still copied"));
    }

    #[test]
    fn paste_shortcut_mapping_matches_platforms() {
        assert_eq!(
            paste_shortcut_for_target_os("macos").modifier,
            PasteModifier::Meta
        );
        assert_eq!(
            paste_shortcut_for_target_os("windows").modifier,
            PasteModifier::Control
        );
        assert_eq!(
            paste_shortcut_for_target_os("linux").modifier,
            PasteModifier::Control
        );
    }

    #[test]
    fn clipboard_errors_do_not_include_private_text() {
        let private_text = unique_private_text();
        let clipboard = FakeClipboard {
            fail_write: true,
            ..Default::default()
        };

        let error = copy_text_to_clipboard_with(&clipboard, &private_text)
            .expect_err("clipboard failure should return an error");

        assert_eq!(error.code, ClipboardErrorCode::ClipboardUnavailable);
        assert!(!error.message.contains(&private_text));
    }

    fn unique_private_text() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos();

        format!("private transcript {nanos}")
    }
}
