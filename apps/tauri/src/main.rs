#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod bridge;
fn main() {
    if std::env::args().any(|arg| arg == "--braintrade-build-id") {
        println!("{}", braintrade_domain::build_identity());
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
