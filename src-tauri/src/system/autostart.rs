use serde::Serialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartAtLoginStatus {
    pub enabled: bool,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartAtLoginError {
    pub code: StartAtLoginErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StartAtLoginErrorCode {
    EnableFailed,
    DisableFailed,
    Unavailable,
}

pub trait AutostartIntegration {
    fn is_enabled(&self) -> Result<bool, ()>;
    fn enable(&self) -> Result<(), ()>;
    fn disable(&self) -> Result<(), ()>;
}

pub struct TauriAutostartIntegration<'a, R: Runtime> {
    app: &'a AppHandle<R>,
}

impl<'a, R: Runtime> TauriAutostartIntegration<'a, R> {
    pub fn new(app: &'a AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> AutostartIntegration for TauriAutostartIntegration<'_, R> {
    fn is_enabled(&self) -> Result<bool, ()> {
        self.app.autolaunch().is_enabled().map_err(|_| ())
    }

    fn enable(&self) -> Result<(), ()> {
        self.app.autolaunch().enable().map_err(|_| ())
    }

    fn disable(&self) -> Result<(), ()> {
        self.app.autolaunch().disable().map_err(|_| ())
    }
}

pub fn get_start_at_login_status_with(
    integration: &impl AutostartIntegration,
) -> Result<StartAtLoginStatus, StartAtLoginError> {
    let enabled = integration
        .is_enabled()
        .map_err(|_| start_at_login_unavailable_error())?;

    Ok(StartAtLoginStatus {
        enabled,
        available: true,
    })
}

pub fn set_start_at_login_enabled_with(
    integration: &impl AutostartIntegration,
    enabled: bool,
) -> Result<StartAtLoginStatus, StartAtLoginError> {
    if enabled {
        integration
            .enable()
            .map_err(|_| start_at_login_enable_error())?;
    } else {
        integration
            .disable()
            .map_err(|_| start_at_login_disable_error())?;
    }

    Ok(StartAtLoginStatus {
        enabled,
        available: true,
    })
}

fn start_at_login_enable_error() -> StartAtLoginError {
    StartAtLoginError {
        code: StartAtLoginErrorCode::EnableFailed,
        message: "Could not enable start at login".to_string(),
    }
}

fn start_at_login_disable_error() -> StartAtLoginError {
    StartAtLoginError {
        code: StartAtLoginErrorCode::DisableFailed,
        message: "Could not disable start at login".to_string(),
    }
}

fn start_at_login_unavailable_error() -> StartAtLoginError {
    StartAtLoginError {
        code: StartAtLoginErrorCode::Unavailable,
        message: "Start at login unavailable".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{
        get_start_at_login_status_with, set_start_at_login_enabled_with, AutostartIntegration,
        StartAtLoginErrorCode, StartAtLoginStatus,
    };

    #[derive(Default)]
    struct FakeAutostart {
        enabled: Cell<bool>,
        fail_status: Cell<bool>,
        fail_enable: Cell<bool>,
        fail_disable: Cell<bool>,
    }

    impl AutostartIntegration for FakeAutostart {
        fn is_enabled(&self) -> Result<bool, ()> {
            if self.fail_status.get() {
                Err(())
            } else {
                Ok(self.enabled.get())
            }
        }

        fn enable(&self) -> Result<(), ()> {
            if self.fail_enable.get() {
                Err(())
            } else {
                self.enabled.set(true);
                Ok(())
            }
        }

        fn disable(&self) -> Result<(), ()> {
            if self.fail_disable.get() {
                Err(())
            } else {
                self.enabled.set(false);
                Ok(())
            }
        }
    }

    #[test]
    fn status_maps_enabled_state() {
        let fake = FakeAutostart::default();
        fake.enabled.set(true);

        assert_eq!(
            get_start_at_login_status_with(&fake).unwrap(),
            StartAtLoginStatus {
                enabled: true,
                available: true
            }
        );
    }

    #[test]
    fn enabling_autostart_returns_enabled_status() {
        let fake = FakeAutostart::default();

        assert_eq!(
            set_start_at_login_enabled_with(&fake, true).unwrap(),
            StartAtLoginStatus {
                enabled: true,
                available: true
            }
        );
        assert!(fake.enabled.get());
    }

    #[test]
    fn disabling_autostart_returns_disabled_status() {
        let fake = FakeAutostart::default();
        fake.enabled.set(true);

        assert_eq!(
            set_start_at_login_enabled_with(&fake, false).unwrap(),
            StartAtLoginStatus {
                enabled: false,
                available: true
            }
        );
        assert!(!fake.enabled.get());
    }

    #[test]
    fn status_failure_maps_to_unavailable_message() {
        let fake = FakeAutostart::default();
        fake.fail_status.set(true);

        let error = get_start_at_login_status_with(&fake).unwrap_err();

        assert_eq!(error.code, StartAtLoginErrorCode::Unavailable);
        assert_eq!(error.message, "Start at login unavailable");
    }

    #[test]
    fn enable_failure_maps_to_safe_message() {
        let fake = FakeAutostart::default();
        fake.fail_enable.set(true);

        let error = set_start_at_login_enabled_with(&fake, true).unwrap_err();

        assert_eq!(error.code, StartAtLoginErrorCode::EnableFailed);
        assert_eq!(error.message, "Could not enable start at login");
    }

    #[test]
    fn disable_failure_maps_to_safe_message() {
        let fake = FakeAutostart::default();
        fake.fail_disable.set(true);

        let error = set_start_at_login_enabled_with(&fake, false).unwrap_err();

        assert_eq!(error.code, StartAtLoginErrorCode::DisableFailed);
        assert_eq!(error.message, "Could not disable start at login");
    }
}
