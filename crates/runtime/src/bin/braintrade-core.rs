fn main() {
    if let Err(error) = braintrade_runtime::run_core() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
