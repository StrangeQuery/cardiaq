//! heartburn: run CARDIAC tapes and decks, assemble them, and see how fast the machine goes.
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;

use heartburn::cardiac::{self, Console, Stop};
use heartburn::tape::{values, Tape};
use heartburn::{asm, cardiasm};

/// Instructions a run may take before it is called a program that never stops.
const LIMIT: u64 = 1_000_000_000;

fn usage() -> &'static str {
    "\
heartburn: CARDIAC, the 1968 Bell Labs cardboard computer, stupid fast

Usage:
  heartburn run FILE.tape [-- INPUT...]
  heartburn run FILE.cardimg [-- INPUT...]
      Run a tape (a program in phases is fed its tapes as it asks for them) or a deck. Each card it punches is
      printed on a line of its own as it is punched. The input is the numbers after --, or else standard input,
      each card read when an INP asks for it. A resource slot on the tape is filled from resources/NAME beside it.
      How it stopped, and after how many instructions, goes to standard error.
  heartburn asm FILE.asm [-o OUT]
      Assemble a listing to a tape that starts at its label `start`: FILE.tape, or OUT.
  heartburn cardiasm FILE.cardiasm
      Jason Whittington's CARDIASM: assemble it to a deck, FILE.cardimg, beside it.
  heartburn bench
      How fast this machine runs CARDIAC: three nested countdown loops, 626.5 million instructions.
"
}

fn die(why: impl std::fmt::Display) -> ! {
    eprintln!("error: {why}");
    std::process::exit(1)
}

fn bad_usage(why: &str) -> ! {
    eprintln!("error: {why}\n\n{}", usage());
    std::process::exit(2)
}

/// The teletype: each card on standard output as it is punched.
struct Teletype(std::io::StdoutLock<'static>);

impl Console for Teletype {
    fn punch(&mut self, card: i32) {
        let _ = writeln!(self.0, "{card}");
        let _ = self.0.flush();
    }
}

/// Standard input as the card reader: each number a card, read when the program asks for one.
fn stdin_cards() -> impl Iterator<Item = i32> {
    let mut lines = std::io::stdin().lock().lines();
    let mut pending: std::vec::IntoIter<i32> = vec![].into_iter();
    std::iter::from_fn(move || loop {
        if let Some(c) = pending.next() {
            return Some(c);
        }
        let line = lines.next()?.unwrap_or_else(|e| die(format!("standard input: {e}")));
        pending = values(&line)
            .unwrap_or_else(|e| die(format!("standard input: {e}")))
            .into_iter();
    })
}

fn run(file: &str, input: Option<Vec<i32>>) {
    let text = std::fs::read_to_string(file).unwrap_or_else(|e| die(format!("{file}: {e}")));
    let mut tape = Tape::parse(&text).unwrap_or_else(|e| die(format!("{file}: {e}")));
    let dir = Path::new(file)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    tape.bind_dir(&dir.join("resources"))
        .unwrap_or_else(|e| die(format!("{file}: {e}")));
    let mut teletype = Teletype(std::io::stdout().lock());
    let (why, steps) = match input {
        Some(cards) => tape.run_into(&cards, &mut std::iter::empty(), LIMIT, &mut teletype),
        None => tape.run_into(&[], &mut stdin_cards(), LIMIT, &mut teletype),
    }
    .unwrap_or_else(|e| die(format!("{file}: {e}")));
    match why {
        Stop::Halt => eprintln!("     halted after {steps} instructions"),
        Stop::OutOfCards => eprintln!("     out of cards after {steps} instructions"),
        Stop::Limit => die(format!("still running after {LIMIT} instructions")),
    }
}

fn assemble(file: &str, output: Option<&str>) {
    let src = std::fs::read_to_string(file).unwrap_or_else(|e| die(format!("{file}: {e}")));
    let p = asm::assemble(3, &src).unwrap_or_else(|e| die(format!("{file}: {e}")));
    let start = *p
        .labels
        .get("start")
        .unwrap_or_else(|| die(format!("{file}: no label start, where the tape starts")));
    let tape = Tape::one(asm::tape(start, &p.cells));
    let out = output
        .map(Path::new)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| Path::new(file).with_extension("tape"));
    let source = Path::new(file).file_name().and_then(|s| s.to_str()).unwrap_or(file);
    let name = out.file_name().and_then(|s| s.to_str()).unwrap_or("tape");
    std::fs::write(
        &out,
        tape.text(&format!("{name}: assembled from {source} by heartburn asm")),
    )
    .unwrap_or_else(|e| die(format!("{}: {e}", out.display())));
    eprintln!("{}: {} cells", out.display(), p.cells.len());
}

fn assemble_cardiasm(file: &str) {
    let src = std::fs::read_to_string(file).unwrap_or_else(|e| die(format!("{file}: {e}")));
    let img = cardiasm::compile(&src).unwrap_or_else(|e| die(format!("{file}: {e}")));
    let out = Path::new(file).with_extension("cardimg");
    std::fs::write(&out, cardiasm::cardimg(&img.deck)).unwrap_or_else(|e| die(format!("{}: {e}", out.display())));
    eprintln!("{}: {} cards", out.display(), img.deck.len());
}

/// The benchmark: three countdown loops, one in another, 500 passes each, the innermost five instructions
/// (CLA c, SUB 00, STO c, TAC, JMP); then OUT a, HRS. 626,503,080 instructions in all, the loader's included.
fn bench() {
    #[rustfmt::skip]
    let cells = [
        (10, 143), (11, 640), (12, 143), (13, 641), (14, 143), (15, 642), (16, 142), (17, 700), (18, 642),
        (19, 321), (20, 816), (21, 141), (22, 700), (23, 641), (24, 326), (25, 814), (26, 140), (27, 700),
        (28, 640), (29, 331), (30, 812), (31, 540), (32, 900), (43, 499),
    ];
    let deck = asm::tape(10, &cells);
    let mut best = f64::MAX;
    let mut steps = 0;
    for _ in 0..5 {
        let mut punched = vec![];
        let t = Instant::now();
        let (stop, n) = cardiac::run(&[&deck], &mut punched, LIMIT);
        let dt = t.elapsed().as_secs_f64();
        assert!(
            stop == Stop::Halt && punched == [-1],
            "the benchmark went wrong: {stop:?}, punched {punched:?}"
        );
        best = best.min(dt);
        steps = n;
    }
    println!(
        "{steps} instructions in {best:.3} s (best of 5): {:.0} million instructions a second, {:.2} ns each",
        steps as f64 / best / 1e6,
        best / steps as f64 * 1e9
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (before, after) = match args.iter().position(|a| a == "--") {
        Some(i) => (&args[..i], Some(&args[i + 1..])),
        None => (&args[..], None),
    };
    match before {
        [c, f] if c == "run" => {
            let input = after.map(|a| values(&a.join(" ")).unwrap_or_else(|e| bad_usage(&e)));
            run(f, input)
        }
        [c, f] if c == "asm" => assemble(f, None),
        [c, f, o, out] if c == "asm" && o == "-o" => assemble(f, Some(out)),
        [c, f] if c == "cardiasm" => assemble_cardiasm(f),
        [c] if c == "bench" => bench(),
        [c] if c == "help" || c == "--help" || c == "-h" => print!("{}", usage()),
        [] => {
            eprint!("{}", usage());
            std::process::exit(2)
        }
        _ => bad_usage("unknown command or arguments"),
    }
}
