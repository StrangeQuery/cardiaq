//! burn: `heartburn::tools::burn` under its own name.
fn main() -> std::process::ExitCode {
    heartburn::tools::burn::main("burn", std::env::args().skip(1).collect())
}
