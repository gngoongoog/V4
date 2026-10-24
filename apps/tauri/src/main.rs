#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod bridge;
fn main() {
    if std::env::args().any(|arg| arg == "--braintrade-build-id") {
        eprintln!("BRAINTRADE_V4_ENTERPRISE_V25_P0O_20261024");
        return;
    }
    if std::env::args().any(|arg| arg == "--core") {
        if let Err(err) = braintrade_runtime::run_core() {
            eprintln!("BRAINTRADE Core failure: {err:#}");
            std::process::exit(1);
        }
        return;
    }
    bridge::run();
}
