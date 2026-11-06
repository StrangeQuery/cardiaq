//! heartburn-tools: the CARDIAC toolchain that will leave this repo for its own: run a listing, make a deck of
//! listings, a listing of a deck. Built only with the `tools` feature, so a release has burn, carc
//! and ekg alone; the tests use it.
use std::io::{BufRead, Write};
use std::path::Path;

use heartburn::cardiac::{self, Console, Stop};
use heartburn::listing::{self, Listing, Notes};
use heartburn::tape::{values, Tape};

/// Instructions a run may take before it is called a program that never stops.
const LIMIT: u64 = 1_000_000_000;

fn usage() -> &'static str {
    "\
heartburn-tools: the CARDIAC toolchain (decks are NAME.cqb, listings NAME.cq)

Usage:
  heartburn-tools run NAME.cqb [-- INPUT...]
      Run a deck: plain cards, one a line. A program in phases is fed its decks as it asks for them:
      when it stops with the bug on n (HRS n), NAME.n.cqb beside it goes in the reader and it starts again at 00.
      Each value it outputs is printed on a line of its own as it is output. The input is the numbers after --, or
      else standard input, each card read when an INP asks for it. A resource slot (NAME.cqb.meta) is filled from
      resources/NAME beside it. How it stopped, and after how many instructions, goes to standard error.
  heartburn-tools run LISTING... [--start NN] [-- INPUT...]
      Run a listing as the 1968 manual or Brian Stuart's page prints it: the first two numbers on a line are the
      address and the contents, the rest is comment (Stuart's six columns assemble a cell whose contents are
      missing). Its cells are written straight into memory and it starts at NN, or at its first instruction (the
      first cell with an opcode other than DATA), or else at the first cell listed. More listings load over the first, as the manual loads a program and then its tables. A deck listed card by card
      (INPUT CARD  CONTENTS) is read in by its loader; at its BLANK card the operator starts it at NN.
  heartburn-tools deck LISTING... [--start NN] [-o OUT]
      A listing to a deck that loads itself and starts at NN (or where run would start it): FILE.cqb, or OUT
      (NAME.2.cqb for a program's deck 2).
  heartburn-tools listing FILE... [--start NN] [-o OUT]
      A program in Brian Stuart's six columns, Address, Contents, Label, Opcode, Operand, Comment, a tab between
      each, under a line saying where it starts: FILE.cq, or OUT. From a deck (each of its decks: NAME.cq,
      NAME.2.cq, ...), or listings in any form heartburn runs (their labels
      and comments kept). The cells a program can run from its start are instructions, the rest DATA, unless the
      source says.
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

/// The teletype: each value on standard output as it is output.
struct Teletype(std::io::StdoutLock<'static>);

impl Console for Teletype {
    fn output(&mut self, card: i32) {
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

/// Whether a file is a deck of cards (else a listing).
fn is_tape(file: &str) -> bool {
    matches!(Path::new(file).extension().and_then(|e| e.to_str()), Some("cqb"))
}

fn report(why: Stop, steps: u64) {
    match why {
        Stop::Halt => eprintln!("     halted after {steps} instructions"),
        Stop::OutOfCards => eprintln!("     out of cards after {steps} instructions"),
        Stop::Limit => die(format!("still running after {LIMIT} instructions")),
    }
}

fn run(file: &str, input: Option<Vec<i32>>) {
    let mut tape = Tape::load(Path::new(file)).unwrap_or_else(|e| die(e));
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
    report(why, steps)
}

/// Read listings, the later loaded over the earlier, their warnings to standard error. A deck stands alone. With
/// them, where the program starts, if the first listing says (its first instruction), and what they say of the
/// cells besides their contents.
fn read_listings(files: &[String]) -> (Listing, Option<usize>, Notes) {
    let mut cells: Vec<(usize, i32)> = vec![];
    let mut unwritten: Vec<(String, usize, String)> = vec![];
    let mut deck = None;
    let mut start = None;
    let mut notes = Notes::default();
    for (i, file) in files.iter().enumerate() {
        let text = std::fs::read_to_string(file).unwrap_or_else(|e| die(format!("{file}: {e}")));
        let read = listing::parse(&text).unwrap_or_else(|e| die(format!("{file}: {e}")));
        for w in &read.warnings {
            eprintln!("warning: {file}: {w}");
        }
        unwritten.extend(read.unwritten.into_iter().map(|(a, what)| (file.clone(), a, what)));
        if i == 0 {
            start = read.start;
        }
        notes.labels.extend(read.notes.labels);
        notes.comments.extend(read.notes.comments);
        notes.data.extend(read.notes.data);
        notes.code.extend(read.notes.code);
        match read.listing {
            Listing::Memory(c) => {
                for (a, v) in c {
                    match cells.iter_mut().find(|x| x.0 == a) {
                        Some(x) => x.1 = v,
                        None => cells.push((a, v)),
                    }
                }
            }
            Listing::Deck(d) => deck = Some(d),
        }
    }
    // a cell a listing leaves to the reader may be in another (the manual's starting pile, say)
    for (file, a, what) in unwritten {
        if !cells.iter().any(|c| c.0 == a) {
            eprintln!("warning: {file}: cell {a:02} holds {what}, not a number: left as it is");
        }
    }
    match deck {
        Some(_) if files.len() > 1 => die("a deck listed card by card is read in alone, not with other listings"),
        Some(d) => (Listing::Deck(d), None, notes),
        None => (Listing::Memory(cells), start, notes),
    }
}

fn run_listing(files: &[String], start: Option<usize>, input: Option<Vec<i32>>) {
    let mut teletype = Teletype(std::io::stdout().lock());
    let mut stdin = stdin_cards();
    let (given, more): (&[i32], &mut dyn Iterator<Item = i32>) = match &input {
        Some(cards) => (cards, &mut std::iter::empty()),
        None => (&[], &mut stdin),
    };
    let mut more = more;
    let (listing, first, _) = read_listings(files);
    match listing {
        Listing::Memory(cells) => {
            let start = start.or(first).unwrap_or(cells[0].0);
            let end = cardiac::run_from(
                &listing::memory(&cells),
                start,
                &[given],
                &mut more,
                &mut teletype,
                LIMIT,
            );
            report(end.stop, end.steps)
        }
        Listing::Deck(d) if !d.blank => {
            if start.is_some() {
                die("--start: this deck has no BLANK card; it starts itself")
            }
            let end = cardiac::run_reading::<false>(&d.cards, &d.data, &[], &mut more, &mut teletype, LIMIT);
            report(end.stop, end.steps)
        }
        Listing::Deck(d) => {
            let Some(start) = start else {
                die("the loading stops at the BLANK card: say where the program starts with --start NN")
            };
            // the loader reads the cards up to the BLANK card, which stops it; then the operator sets the bug
            let load = cardiac::run_reading::<false>(&d.cards, &[], &[], &mut std::iter::empty(), &mut teletype, LIMIT);
            if load.stop != Stop::OutOfCards {
                return report(load.stop, load.steps);
            }
            let end = cardiac::run_from(&load.memory, start, &[&d.data, given], &mut more, &mut teletype, LIMIT);
            report(end.stop, load.steps + end.steps)
        }
    }
}

fn tape(files: &[String], start: Option<usize>, output: Option<&str>) {
    let (read, first, _) = read_listings(files);
    let cards = match read {
        Listing::Memory(cells) => listing::deck(start.or(first).unwrap_or(cells[0].0), &cells)
            .unwrap_or_else(|e| die(format!("{}: {e}", files[0]))),
        Listing::Deck(d) => {
            let mut c = d.cards;
            match (d.blank, start) {
                (true, Some(s)) => c.extend([2, 800 + s as i32]),
                (true, None) => {
                    die("the loading stops at the BLANK card: say where the program starts with --start NN")
                }
                (false, Some(_)) => die("--start: this deck has no BLANK card; it starts itself"),
                (false, None) => {}
            }
            c.extend(d.data);
            c
        }
    };
    let out = output
        .map(Path::new)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| Path::new(&files[0]).with_extension("cqb"));
    std::fs::write(&out, Tape::deck_text(&cards)).unwrap_or_else(|e| die(format!("{}: {e}", out.display())));
    eprintln!("{}: {} cards", out.display(), cards.len());
}

/// Write `text` to `out`, unless that is one of the files it was made from.
fn write_new(out: &Path, text: &str, from: &[String]) {
    if from.iter().any(|f| Path::new(f) == out) {
        die(format!(
            "{}: that is the file it is made from; say where it goes with -o",
            out.display()
        ))
    }
    std::fs::write(out, text).unwrap_or_else(|e| die(format!("{}: {e}", out.display())));
}

/// `heartburn listing`: decks or listings in Brian Stuart's six columns.
fn listing_of(files: &[String], start: Option<usize>, output: Option<&str>) {
    let out = output
        .map(Path::new)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| Path::new(&files[0]).with_extension("cq"));
    let one = |path: &Path, cells: &[(usize, i32)], at: Option<usize>, notes: &Notes| {
        if at.is_none() {
            eprintln!(
                "{}: where it starts isn't known: every cell is DATA (say where with --start)",
                path.display()
            );
        }
        write_new(path, &listing::six_columns(cells, at, notes), files);
        eprintln!("{}: {} cells", path.display(), cells.len());
    };
    match files {
        [f] if is_tape(f) => {
            let tape = Tape::load(Path::new(f)).unwrap_or_else(|e| die(e));
            if start.is_some() && tape.phases.len() > 1 {
                bad_usage("--start: a program in phases starts each tape where its loader says")
            }
            for (i, cards) in tape.phases.iter().enumerate() {
                let listing::Loaded { cells, start: at, left } =
                    listing::from_deck(cards).unwrap_or_else(|e| die(format!("{f}: tape {}: {e}", i + 1)));
                if !left.is_empty() {
                    eprintln!(
                        "{f}: tape {}: {} cards after the program, its input: not in the listing",
                        i + 1,
                        left.len()
                    );
                }
                one(&Tape::phase_path(&out, i + 1), &cells, start.or(at), &Notes::default());
            }
        }
        _ if files.iter().any(|f| is_tape(f)) => bad_usage("one deck at a time, and not with listings"),
        _ => match read_listings(files) {
            (Listing::Memory(cells), first, notes) => one(&out, &cells, start.or(first), &notes),
            (Listing::Deck(d), _, _) => {
                let listing::Loaded { cells, start: at, .. } =
                    listing::from_deck(&d.cards).unwrap_or_else(|e| die(format!("{}: {e}", files[0])));
                one(&out, &cells, start.or(at), &Notes::default())
            }
        },
    }
}

/// The files and options of `run` and `deck`: `--start NN` and `-o OUT` anywhere among the files.
fn files_and_options(args: &[String]) -> (Vec<String>, Option<usize>, Option<String>) {
    let (mut files, mut start, mut out) = (vec![], None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--start" => {
                let n = it.next().unwrap_or_else(|| bad_usage("--start needs a cell, 00 to 99"));
                start = Some(
                    n.parse::<usize>()
                        .ok()
                        .filter(|&s| s < 100)
                        .unwrap_or_else(|| bad_usage(&format!("--start {n}: a cell is 00 to 99"))),
                );
            }
            "-o" => out = Some(it.next().unwrap_or_else(|| bad_usage("-o needs a file")).clone()),
            f if f.starts_with('-') => bad_usage(&format!("unknown option {f}")),
            f => files.push(f.to_string()),
        }
    }
    if files.is_empty() {
        bad_usage("no file")
    }
    (files, start, out)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (before, after) = match args.iter().position(|a| a == "--") {
        Some(i) => (&args[..i], Some(&args[i + 1..])),
        None => (&args[..], None),
    };
    match before {
        [c, rest @ ..] if c == "run" => {
            let input = after.map(|a| values(&a.join(" ")).unwrap_or_else(|e| bad_usage(&e)));
            let (files, start, out) = files_and_options(rest);
            if out.is_some() {
                bad_usage("-o is for deck")
            }
            match files.as_slice() {
                [f] if is_tape(f) => {
                    if start.is_some() {
                        bad_usage("--start is for listings: a deck starts itself")
                    }
                    run(f, input)
                }
                _ if files.iter().any(|f| is_tape(f)) => bad_usage("one deck at a time, and not with listings"),
                _ => run_listing(&files, start, input),
            }
        }
        [c, rest @ ..] if c == "deck" && after.is_none() => {
            let (files, start, out) = files_and_options(rest);
            if files.iter().any(|f| is_tape(f)) {
                bad_usage("deck makes a deck of listings")
            }
            tape(&files, start, out.as_deref())
        }
        [c, rest @ ..] if c == "listing" && after.is_none() => {
            let (files, start, out) = files_and_options(rest);
            listing_of(&files, start, out.as_deref())
        }
        [c] if c == "help" || c == "--help" || c == "-h" => print!("{}", usage()),
        [] => {
            eprint!("{}", usage());
            std::process::exit(2)
        }
        _ => bad_usage("unknown command or arguments"),
    }
}
