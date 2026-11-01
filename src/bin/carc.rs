//! carc: assemble a CARDIAC listing into a deck. `burn` runs decks; carc makes them, from a listing in Brian
//! Stuart's six columns, with the assembler Cardigan uses (`heartburn::assemble`), so a listing assembles the same
//! here and in the IDE.
use std::path::Path;

use heartburn::assemble::{self, Assembly};

fn usage() -> &'static str {
    "\
carc: assemble a CARDIAC listing into a deck

Usage:
  carc NAME.cardiac [-o OUT.deck] [--start NN]
      Assemble a listing, one cell a line: ADDRESS CONTENTS LABEL OPCODE OPERAND COMMENT, Brian Stuart's six
      columns (the contents may be left out: they are worked out from the opcode and operand), or the manual's
      ADDRESS CONTENTS COMMENT. Lines that aren't cells (titles, prose, ; and # comments) are skipped.
      The deck goes to NAME.deck beside it, or OUT.deck: the loader's 002 and 800, an address card and a
      contents card for each cell, then 002 8NN to start the program at NN. It starts at --start NN, or else
      where the listing says: a \"Start at NN.\" line, the line labelled start, its first instruction.
      What is wrong is reported as FILE:LINE:COLUMN: error|warning: WHAT. A line with an error isn't loaded,
      and no deck is written while there are errors.
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

/// What is wrong, compiler style, a line each; whether any of it is an error.
fn report(file: &str, text: &str, asm: &Assembly) -> bool {
    let lines = assemble::lines(text);
    for d in &asm.diagnostics {
        let col = lines[d.line][..d.span.start].chars().count() + 1;
        let kind = if d.warning { "warning" } else { "error" };
        eprintln!("{file}:{}:{col}: {kind}: {}", d.line + 1, d.message);
    }
    asm.diagnostics.iter().any(|d| !d.warning)
}

fn carc(file: &str, out: Option<&str>, start: Option<usize>) {
    let text = std::fs::read_to_string(file).unwrap_or_else(|e| die(format!("{file}: {e}")));
    let asm = assemble::assemble(&text);
    let errors = report(file, &text, &asm);
    // the loader's own cells: 01 and 02 are the loader, and its jumps write 99
    for r in &asm.rows {
        let why = match r.addr {
            1 | 2 => "the loader's: it is not on the deck",
            99 => "written by the loader's jumps: what the deck puts there doesn't stay",
            _ => continue,
        };
        eprintln!("{file}:{}:1: warning: cell {:02} is {why}", r.line + 1, r.addr);
    }
    if errors {
        die(format!("{file}: not assembled: no deck written"))
    }
    let Some(start) = start.or(asm.start) else {
        die(format!("{file}: no cells: nothing to put on a deck"))
    };
    let deck = match out {
        Some(o) => o.to_string(),
        None => Path::new(file).with_extension("deck").to_string_lossy().into_owned(),
    };
    let name = |p: &str| {
        Path::new(p)
            .file_name()
            .map_or(p.to_string(), |n| n.to_string_lossy().into_owned())
    };
    let text = format!(
        "# {}: {} on cards, for the loader; it starts at {start:02}\n# CARDIAC cards, one per line, read from the \
         top. The program's input goes after them.\n{}",
        name(&deck),
        name(file),
        assemble::deck_text(&assemble::deck(&asm, start))
    );
    std::fs::write(&deck, text).unwrap_or_else(|e| die(format!("{deck}: {e}")));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut file, mut out, mut start) = (None, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "help" | "--help" | "-h" => {
                print!("{}", usage());
                return;
            }
            "-o" => out = Some(it.next().unwrap_or_else(|| bad_usage("-o needs a file")).as_str()),
            "--start" => {
                let n = it.next().unwrap_or_else(|| bad_usage("--start needs a cell"));
                let cell = n.parse().ok().filter(|&c: &usize| c < 100 && n.len() <= 2);
                start = Some(cell.unwrap_or_else(|| bad_usage(&format!("--start {n}: a cell is 00 to 99"))));
            }
            f if f.starts_with('-') => bad_usage(&format!("unknown option {f}")),
            f if file.is_none() => file = Some(f),
            _ => bad_usage("one listing at a time"),
        }
    }
    match file {
        Some(f) => carc(f, out, start),
        None => {
            eprint!("{}", usage());
            std::process::exit(2)
        }
    }
}
