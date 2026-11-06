//! carc: `heartburn::tools::carc` under its own name.
fn main() -> std::process::ExitCode {
    heartburn::tools::carc::main("carc", std::env::args().skip(1).collect())
}
