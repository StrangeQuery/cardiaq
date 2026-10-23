//! Load a CARDIAC program from a listing, make it a tape, and run it: Program No. 1 of the 1968 manual, which reads
//! two cards and punches their sum.
use heartburn::listing::{self, Listing};
use heartburn::tape::Tape;

const PROGRAM: &str = "\
ADDRESS  CONTENTS  COMMENTS
  17       034     Read \"A\".
  18       035     Read \"B\".
  19       134     Clear accumulator and add \"A\".
  20       235     Add \"B\" (\"S\" is now in accumulator).
  21       636     Store \"S\".
  22       536     Print \"S\".
  23       900     Halt and reset.
";

fn main() -> Result<(), String> {
    let Listing::Memory(cells) = listing::parse(PROGRAM)?.listing else {
        return Err("not a memory listing".into());
    };
    let tape = Tape::one(listing::deck(17, &cells)?);
    let (punched, why, steps) = tape.run(&[2, 3], 1_000)?;
    println!("{punched:?} ({why:?} after {steps} instructions)");
    assert_eq!(punched, [5]);
    Ok(())
}
