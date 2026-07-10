// On Windows, always use the GUI subsystem to prevent a console window
// from flashing (especially during autostart / registry Run keys).
// This applies to all builds (not just release) so that debug builds
// launched outside a terminal produce no console flash.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    // On Windows, detach from any inherited console at the earliest
    // possible point. This is a belt-and-suspenders measure alongside
    // the windows_subsystem attribute above — if the CRT or an embedded
    // resource ever causes a console to attach, FreeConsole removes it
    // before any I/O can make it visible.
    #[cfg(target_os = "windows")]
    {
        // Safety: FreeConsole is a safe Win32 call that detaches this
        // process from its console.  It returns zero if there is no
        // console to detach from, which we treat as success.
        let _ = unsafe { windows::Win32::System::Console::FreeConsole() };
    }

    std::panic::set_hook(Box::new(|panic_info| {
        let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };

        let location = panic_info
            .location()
            .map(|l| l.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // On non-Windows platforms write to stderr as a hard fallback.
        // On Windows the binary is always a GUI-subsystem app, so
        // stderr is never attached — writing to it can never flash a
        // console.  The log::error! path below covers both file-based
        // diagnostics (once initialised) and silent no-ops otherwise.
        #[cfg(not(target_os = "windows"))]
        eprintln!("[floe] PANIC: {msg} at {location}");

        // Route through the diagnostics logger if it is initialized.
        log::error!("[PANIC] {msg} at {location}");
    }));

    floe_lib::run();
}
