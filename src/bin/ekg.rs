//! ekg: `heartburn::tools::ekg` under its own name.
fn main() -> std::process::ExitCode {
    heartburn::tools::ekg::main("ekg", std::env::args().skip(1).collect())
}
