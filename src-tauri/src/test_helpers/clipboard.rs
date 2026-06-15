use std::sync::Mutex;

use crate::commands::clipboard::{
    ClipboardError, ClipboardErrorCode, PasteShortcut, PasteSimulator, TextClipboard,
};

pub struct FakeClipboard {
    pub text: Mutex<Option<String>>,
    pub fail_write: bool,
    pub read_back: Mutex<Option<String>>,
}

impl FakeClipboard {
    pub fn new() -> Self {
        Self {
            text: Mutex::new(None),
            fail_write: false,
            read_back: Mutex::new(None),
        }
    }

    pub fn text(&self) -> Option<String> {
        self.text.lock().unwrap().clone()
    }
}

impl TextClipboard for FakeClipboard {
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        if self.fail_write {
            return Err(ClipboardError {
                domain: "clipboard",
                code: ClipboardErrorCode::ClipboardUnavailable,
                message: "Floe could not write to the clipboard.".to_string(),
            });
        }
        *self.text.lock().unwrap() = Some(text.to_string());
        match self.read_back.lock().unwrap().as_ref() {
            Some(read_back) if read_back == text => Ok(()),
            Some(_) => Err(ClipboardError {
                domain: "clipboard",
                code: ClipboardErrorCode::ClipboardUnavailable,
                message: "Floe could not write to the clipboard.".to_string(),
            }),
            None => Ok(()),
        }
    }
}

pub struct FakePasteSimulator {
    pub shortcuts: Mutex<Vec<PasteShortcut>>,
    pub fail_paste: bool,
}

impl FakePasteSimulator {
    pub fn new() -> Self {
        Self {
            shortcuts: Mutex::new(Vec::new()),
            fail_paste: false,
        }
    }

    pub fn shortcut_count(&self) -> usize {
        self.shortcuts.lock().unwrap().len()
    }
}

impl PasteSimulator for FakePasteSimulator {
    fn paste_shortcut(&self, shortcut: PasteShortcut) -> Result<(), ClipboardError> {
        self.shortcuts.lock().unwrap().push(shortcut);
        if self.fail_paste {
            return Err(ClipboardError {
                domain: "clipboard",
                code: ClipboardErrorCode::PasteUnavailable,
                message: "Transcript copied to clipboard, but Floe could not send the paste shortcut. Paste manually with Command+V or Control+V.".to_string(),
            });
        }
        Ok(())
    }
}
