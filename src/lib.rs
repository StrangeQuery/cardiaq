//! HEARTBURN: CARDIAC, the 1968 Bell Labs cardboard computer, stupid fast. The machine (`cardiac`), its tapes and
//! the `.tape` file they are kept in (`tape`), listings as the 1968 manual and Brian Stuart print them (`listing`),
//! and Jason Whittington's CARDIASM (`cardiasm`), ported from C#.
//!
//! The machine is standard CARDIAC, as the 1968 manual and Brian Stuart's simulator define it: 100 cells of three
//! decimal digits and a sign, cell 00 wired to 001, a four-digit accumulator, ten instructions. Fast is the point;
//! everything a program can see is exactly the cardboard machine's.
pub mod cardiac;
pub mod cardiasm;
pub mod listing;
pub mod tape;
