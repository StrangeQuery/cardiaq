//! Assemble a CARDIAC program, make it a tape, and run it: reads two cards, punches their sum.
use heartburn::{asm, tape::Tape};

fn main() -> Result<(), String> {
    let program = asm::assemble(
        3,
        "start: INP a\n INP b\n CLA a\n ADD b\n STO a\n OUT a\n HRS 0\na: DAT 0\nb: DAT 0\n",
    )?;
    let tape = Tape::one(asm::tape(program.label("start"), &program.cells));
    let (punched, why, steps) = tape.run(&[2, 3], 1_000)?;
    println!("{punched:?} ({why:?} after {steps} instructions)");
    assert_eq!(punched, [5]);
    Ok(())
}
