//! HEARTBURN: CARDIAC, stupid fast. The machine (`cardiac`), its decks and
//! the `.cqb` files they are kept in (`tape`), listings as the 1968 manual and Brian Stuart print them (`listing`),
//! the assembler `carc` and Cardigan use, a six-column listing made into cells and a deck (`assemble`), and the
//! panic maps a compiler writes beside a deck (`panics`).
//!
//! The machine is standard CARDIAC, as the 1968 manual and Brian Stuart's simulator define it: 100 cells of three
//! decimal digits and a sign, cell 00 wired to 001, a four-digit accumulator, ten instructions. Fast is the point;
//! everything a program can see is exactly the cardboard machine's.
pub mod assemble;
pub mod cardiac;
pub mod listing;
pub mod panics;
pub mod tape;
#[cfg(feature = "tools")]
pub mod tools;
