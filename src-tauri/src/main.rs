#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if floe_lib::maybe_run_mock_asr_sidecar_from_args() {
        return;
    }

    floe_lib::run();
}
