//! The programs: `burn` runs a deck, `carc` assembles a listing into one, `ekg` debugs and profiles a program.
//! Each is a `main(prog, args)`, exactly the program, given its arguments and the name it goes by in its messages
//! and usage: heartburn's own `burn`, `carc` and `ekg` are those with their own names, and cargo-cardiac's
//! `cargo cardiac run`, `asm` and `debug` are the same with theirs. Built only with the `tools` feature.
pub mod burn;
pub mod carc;
pub mod ekg;

mod cli;
mod getopt;
