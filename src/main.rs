//! burn: run a CARDIAC deck, and see how fast this machine runs CARDIAC. It only runs things: decks are made from
//! listings by carc (or Cardigan's Export). A file is a deck if it looks like one: it starts with the loader.
use std::io::{Read, Write};
use std::path::Path;
use std::time::Instant;

use heartburn::cardiac::{self, Console, Stop};
use heartburn::listing;
use heartburn::tape::values;

mod cli;
mod getopt;

/// Instructions a run may take before it is called a program that never stops (unless --limit says).
const LIMIT: u64 = 1_000_000_000;

const PROG: &str = "burn";

fn usage() -> &'static str {
    "\
Usage: burn [OPTION]... DECK [--] [INPUT]...
  or:  burn bench
Run a CARDIAC deck.

A deck is a text file with one card per line. Lines starting with # are comments. The deck has to start with the
loader's cards, 002 and 800, but the file can have any name. carc makes a deck from a listing. When DECK is -,
the deck is read from standard input.

The program's input is the INPUT numbers after DECK, if there are any or if there is a --. Otherwise the input
comes from standard input, and each card is read when an INP asks for it. Every value the program outputs is
printed on its own line as soon as it is output. How the program stopped, and after how many instructions, goes
to standard error.

A program that comes in several decks is given each deck when it asks for it. When the program halts with the
program counter on n (HRS n), burn puts NAME.n.cqb, from the same folder, in the reader and starts again at 00.

burn bench measures how fast this machine runs CARDIAC, using three nested countdown loops (626.5 million
instructions). To run a deck that is actually called bench, use ./bench.

      --limit=N  stop a program after N instructions (the default is 1000000000)
  -h, --help     display this help and exit
      --version  output version information and exit

The exit status is 0 if the program halted or ran out of input cards, 1 if the deck can't be read or the
program doesn't stop, and 2 if there is a mistake on the command line.
"
}

fn die(why: impl std::fmt::Display) -> ! {
    getopt::die(PROG, why)
}

/// The teletype: each value on standard output as it is output. When no one reads it any more (`burn deck | head`)
/// the program is stopped, as a C program is by SIGPIPE, rather than left running to the limit.
struct Teletype(std::io::StdoutLock<'static>);

impl Console for Teletype {
    fn output(&mut self, card: i32) {
        if let Err(e) = writeln!(self.0, "{card}").and_then(|_| self.0.flush()) {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                std::process::exit(0)
            }
            die(format!("standard output: {}", getopt::os_error(&e)))
        }
    }
}

fn run(file: &str, input: Option<Vec<i32>>, limit: u64) {
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
        let deck = cli::deck_or_not(name, &text, None).unwrap_or_else(|e| die(e));
        let deck = deck.unwrap_or_else(|| not_a_deck(name));
        (deck, Path::new("."), Some(input.unwrap_or_default()))
    } else {
        // read for what the OS says is wrong (no such file, a directory, no permission), and what it is
        let text = std::fs::read_to_string(file).unwrap_or_else(|e| die(format!("{file}: {}", getopt::os_error(&e))));
        let deck = cli::deck_or_not(file, &text, Some(Path::new(file))).unwrap_or_else(|e| die(e));
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
        Some(cards) => deck.run_into(&cards, &mut std::iter::empty(), limit, &mut teletype),
        None => deck.run_into(&[], &mut cli::stdin_cards(PROG), limit, &mut teletype),
    }
    .unwrap_or_else(|e| die(format!("{name}: {e}")));
    match why {
        Stop::Halt => eprintln!("     halted after {steps} instructions"),
        Stop::OutOfCards => eprintln!("     out of cards after {steps} instructions"),
        Stop::Limit => die(format!(
            "still running after {limit} instructions (--limit N to run longer)"
        )),
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
        let mut outputs = vec![];
        let t = Instant::now();
        let (stop, n) = cardiac::run(&[&deck], &mut outputs, LIMIT);
        let dt = t.elapsed().as_secs_f64();
        assert!(
            stop == Stop::Halt && outputs == [-1],
            "the benchmark went wrong: {stop:?}, output {outputs:?}"
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
        getopt::Opt {
            short: None,
            long: "limit",
            arg: true,
        },
    ];
    // options come before the deck: what follows it is the program's input, -5 a card, not an option
    let (given, operands) = getopt::parse(PROG, &opts, std::env::args().skip(1), true);
    let mut limit = LIMIT;
    for (o, value) in &given {
        match *o {
            "help" => return print!("{}", usage()),
            "version" => return getopt::version(PROG),
            _ => {
                let v = value.as_deref().unwrap_or_default();
                limit = v.parse().ok().filter(|&n| n > 0).unwrap_or_else(|| {
                    getopt::usage_error(PROG, format!("invalid --limit '{v}': a number of instructions"))
                });
            }
        }
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
            run(deck, input, limit)
        }
    }
}
