//! burn: run a CARDIAC deck, and see how fast this machine runs CARDIAC. It only runs things: decks are made from
//! listings by carc (or Cardigan's Export). A file is a deck if it looks like one: it starts with the loader.
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::time::Instant;

use heartburn::cardiac::{self, Console, Stop};
use heartburn::listing;
use heartburn::tape::{values, Tape};

mod getopt;

/// Instructions a run may take before it is called a program that never stops.
const LIMIT: u64 = 1_000_000_000;

const PROG: &str = "burn";

fn usage() -> &'static str {
    "\
Usage: burn [OPTION]... DECK [--] [INPUT]...
  or:  burn bench
Run a CARDIAC deck: CARDIAC, the 1968 Bell Labs cardboard computer, stupid fast.

A deck is cards, one a line (lines starting with # are comments), starting with the loader's 002 and 800,
whatever the file is called; carc makes one from a listing. With DECK -, the deck is read from standard input.
The program's input is the INPUT numbers after DECK, if there are any or a --, or else standard input, each card
read when an INP asks for it. Each card it punches is printed on a line of its own as it is punched. How it
stopped, and after how many instructions, goes to standard error.
A program in several decks is fed them as it asks: when it stops with the PC on n (HRS n), NAME.n.deck beside it
goes in the reader and it starts again at 00.

burn bench: how fast this machine runs CARDIAC, three nested countdown loops, 626.5 million instructions. (A deck
called bench runs as ./bench.)

  -h, --help     display this help and exit
      --version  output version information and exit

Exit status is 0 if the program halted or ran out of cards, 1 if the deck can't be read or the program doesn't
stop, 2 for a mistake on the command line.
"
}

fn die(why: impl std::fmt::Display) -> ! {
    getopt::die(PROG, why)
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
        let line = lines
            .next()?
            .unwrap_or_else(|e| die(format!("standard input: {}", getopt::os_error(&e))));
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
    let name = if file == "-" { "standard input" } else { file };
    // what a file is, from what's in it, not its name: a deck starts with the loader
    let not_a_deck = |name: &str| -> ! {
        die(format!(
            "{name}: not a CARDIAC deck (a deck starts with the loader's cards, 002 and 800): make one from a \
             listing with carc"
        ))
    };
    let (mut deck, dir, input) = if file == "-" {
        // the whole of standard input is the deck (and any cards after it), so the input is what comes after DECK
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .unwrap_or_else(|e| die(format!("standard input: {}", getopt::os_error(&e))));
        let deck = Tape::parse(&text).ok().filter(looks_like_a_deck);
        let deck = deck.unwrap_or_else(|| not_a_deck(name));
        (deck, Path::new("."), Some(input.unwrap_or_default()))
    } else {
        // read once for what the OS says is wrong (no such file, a directory, no permission)
        std::fs::read(file).unwrap_or_else(|e| die(format!("{file}: {}", getopt::os_error(&e))));
        let deck = Tape::load(Path::new(file)).ok().filter(looks_like_a_deck);
        let dir = Path::new(file)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        (deck.unwrap_or_else(|| not_a_deck(file)), dir, input)
    };
    deck.bind_dir(&dir.join("resources"))
        .unwrap_or_else(|e| die(format!("{name}: {e}")));
    let mut teletype = Teletype(std::io::stdout().lock());
    let (why, steps) = match input {
        Some(cards) => deck.run_into(&cards, &mut std::iter::empty(), LIMIT, &mut teletype),
        None => deck.run_into(&[], &mut stdin_cards(), LIMIT, &mut teletype),
    }
    .unwrap_or_else(|e| die(format!("{name}: {e}")));
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
    let opts = [
        getopt::Opt {
            short: Some('h'),
            long: "help",
            arg: false,
        },
        getopt::Opt {
            short: None,
            long: "version",
            arg: false,
        },
    ];
    // options come before the deck: what follows it is the program's input, -5 a card, not an option
    let (given, operands) = getopt::parse(PROG, &opts, std::env::args().skip(1), true);
    if let Some((o, _)) = given.first() {
        match *o {
            "help" => print!("{}", usage()),
            _ => getopt::version(PROG),
        }
        return;
    }
    match operands.split_first() {
        None => getopt::usage_error(PROG, "missing deck operand"),
        // `burn help`, as burn always took it
        Some((c, [])) if c == "help" => print!("{}", usage()),
        Some((c, [])) if c == "bench" => bench(),
        Some((c, [extra, ..])) if c == "bench" => getopt::usage_error(PROG, format!("extra operand '{extra}'")),
        Some((deck, rest)) => {
            let (dashes, rest) = match rest {
                [d, r @ ..] if d == "--" => (true, r),
                r => (false, r),
            };
            let input = (dashes || !rest.is_empty()).then(|| {
                values(&rest.join(" ")).unwrap_or_else(|e| getopt::usage_error(PROG, format!("invalid input: {e}")))
            });
            run(deck, input)
        }
    }
}
