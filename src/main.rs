//! burn: run a CARDIAC deck, and see how fast this machine runs CARDIAC. It only runs things: decks are made from
//! listings elsewhere (CARDIAC Code's Export). A file is a deck if it looks like one: it starts with the loader.
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;

use heartburn::cardiac::{self, Console, Stop};
use heartburn::listing;
use heartburn::tape::{values, Tape};

/// Instructions a run may take before it is called a program that never stops.
const LIMIT: u64 = 1_000_000_000;

fn usage() -> &'static str {
    "\
burn: CARDIAC, the 1968 Bell Labs cardboard computer, stupid fast

Usage:
  burn NAME.deck [-- INPUT...]
      Run a deck: cards, one a line (lines starting with # are comments), starting with the loader's
      002 and 800 (whatever the file is called). The input is the numbers after --, or
      else standard input, each card read when an INP asks for it. Each card it punches is printed on a line of
      its own as it is punched. How it stopped, and after how many instructions, goes to standard error.
      A program in several decks is fed them as it asks: when it stops with the PC on n (HRS n), NAME.n.deck
      beside it goes in the reader and it starts again at 00.
  burn bench
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

/// Whether cards look like a deck: they start with the loader's two cards, 002 and 800, as every deck does.
fn looks_like_a_deck(deck: &Tape) -> bool {
    deck.phases.first().is_some_and(|cards| cards.starts_with(&[2, 800]))
}

fn run(file: &str, input: Option<Vec<i32>>) {
    // what a file is, from what's in it, not its name: a deck starts with the loader
    if !Path::new(file).is_file() {
        die(format!("{file}: no such file"))
    }
    let mut deck = Tape::load(Path::new(file))
        .ok()
        .filter(looks_like_a_deck)
        .unwrap_or_else(|| {
            die(format!(
                "{file}: not a CARDIAC deck (a deck starts with the loader's cards, 002 and 800): make one from a \
                 listing in CARDIAC Code"
            ))
        });
    let dir = Path::new(file)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    deck.bind_dir(&dir.join("resources"))
        .unwrap_or_else(|e| die(format!("{file}: {e}")));
    let mut teletype = Teletype(std::io::stdout().lock());
    let (why, steps) = match input {
        Some(cards) => deck.run_into(&cards, &mut std::iter::empty(), LIMIT, &mut teletype),
        None => deck.run_into(&[], &mut stdin_cards(), LIMIT, &mut teletype),
    }
    .unwrap_or_else(|e| die(format!("{file}: {e}")));
    match why {
        Stop::Halt => eprintln!("     halted after {steps} instructions"),
        Stop::OutOfCards => eprintln!("     out of cards after {steps} instructions"),
        Stop::Limit => die(format!("still running after {LIMIT} instructions")),
    }
}

/// The benchmark: three countdown loops, one in another, 500 passes each, the innermost five instructions
/// (CLA c, SUB 00, STO c, TAC, JMP); then OUT a, HRS. 626,503,081 instructions in all, the loader's included.
fn bench() {
    #[rustfmt::skip]
    let cells = [
        (10, 143), (11, 640), (12, 143), (13, 641), (14, 143), (15, 642), (16, 142), (17, 700), (18, 642),
        (19, 321), (20, 816), (21, 141), (22, 700), (23, 641), (24, 326), (25, 814), (26, 140), (27, 700),
        (28, 640), (29, 331), (30, 812), (31, 540), (32, 900), (43, 499),
    ];
    let deck = listing::deck(10, &cells).unwrap();
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
        [c] if c == "bench" && after.is_none() => bench(),
        [c] if c == "help" || c == "--help" || c == "-h" => print!("{}", usage()),
        [f] if f.starts_with('-') => bad_usage(&format!("unknown option {f}")),
        [f] => {
            let input = after.map(|a| values(&a.join(" ")).unwrap_or_else(|e| bad_usage(&e)));
            run(f, input)
        }
        [] => {
            eprint!("{}", usage());
            std::process::exit(2)
        }
        _ => bad_usage("one deck at a time"),
    }
}
