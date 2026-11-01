//! carc: assemble a CARDIAC listing into a deck. `burn` runs decks; carc makes them, from a listing in Brian
//! Stuart's six columns, with the assembler Cardigan uses (`heartburn::assemble`), so a listing assembles the same
//! here and in the IDE.
use std::io::{Read, Write};
use std::path::Path;

use heartburn::assemble::{self, Assembly};

#[path = "../getopt.rs"]
mod getopt;

const PROG: &str = "carc";

fn usage() -> &'static str {
    "\
Usage: carc [OPTION]... FILE
Assemble a CARDIAC listing into a deck for burn.

The listing has a cell a line: ADDRESS CONTENTS LABEL OPCODE OPERAND COMMENT, Brian Stuart's six columns (the
contents may be left out: they are worked out from the opcode and operand), or the manual's ADDRESS CONTENTS
COMMENT. Lines that aren't cells (titles, prose, ; and # comments) are skipped. With FILE -, the listing is read
from standard input.
The deck is the loader's 002 and 800, an address card and a contents card for each cell, then 002 8NN to start
the program at NN: where --start-at says, or else where the listing says (a \"Start at NN.\" line, the line labelled
start, its first instruction).

  -o, --output=FILE  write the deck to FILE (- for standard output); by default NAME.deck beside
                       NAME.cardiac, or standard output for a listing read from standard input
      --start-at=NN  start the program at cell NN
  -h, --help         display this help and exit
      --version      output version information and exit

What is wrong is reported as FILE:LINE:COLUMN: error: WHAT (or warning: WHAT). A line with an error isn't
loaded, and no deck is written while there are errors.

Exit status is 0 if the deck was written, 1 if the listing has errors or a file can't be read or written, 2 for
a mistake on the command line.
"
}

fn die(why: impl std::fmt::Display) -> ! {
    getopt::die(PROG, why)
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
    // FILE - is standard input, named <stdin> in what is reported, as gcc names it
    let (text, listing) = if file == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .unwrap_or_else(|e| die(format!("standard input: {}", getopt::os_error(&e))));
        (text, "<stdin>")
    } else {
        let text = std::fs::read_to_string(file).unwrap_or_else(|e| die(format!("{file}: {}", getopt::os_error(&e))));
        (text, file)
    };
    let asm = assemble::assemble(&text);
    let errors = report(listing, &text, &asm);
    // the loader's own cells: 01 and 02 are the loader, and its jumps write 99
    for r in &asm.rows {
        let why = match r.addr {
            1 | 2 => "the loader's: it is not on the deck",
            99 => "written by the loader's jumps: what the deck puts there doesn't stay",
            _ => continue,
        };
        eprintln!("{listing}:{}:1: warning: cell {:02} is {why}", r.line + 1, r.addr);
    }
    if errors {
        die(format!("{listing}: not assembled: no deck written"))
    }
    let Some(start) = start.or(asm.start) else {
        die(format!("{listing}: no cells: nothing to put on a deck"))
    };
    let deck = match out {
        Some(o) => o.to_string(),
        None if file == "-" => "-".to_string(),
        None => Path::new(file).with_extension("deck").to_string_lossy().into_owned(),
    };
    let name = |p: &str| {
        Path::new(p)
            .file_name()
            .map_or(p.to_string(), |n| n.to_string_lossy().into_owned())
    };
    let title = match deck.as_str() {
        "-" => String::new(),
        d => format!("{}: ", name(d)),
    };
    let text = format!(
        "# {title}{} on cards, for the loader; it starts at {start:02}\n# CARDIAC cards, one per line, read from the \
         top. The program's input goes after them.\n{}",
        name(listing),
        assemble::deck_text(&assemble::deck(&asm, start))
    );
    let written = match deck.as_str() {
        "-" => std::io::stdout().lock().write_all(text.as_bytes()),
        d => std::fs::write(d, text),
    };
    written.unwrap_or_else(|e| match deck.as_str() {
        "-" => die(format!("standard output: {}", getopt::os_error(&e))),
        d => die(format!("{d}: {}", getopt::os_error(&e))),
    });
}

fn main() {
    let opts = [
        getopt::Opt {
            short: Some('o'),
            long: "output",
            arg: true,
        },
        getopt::Opt {
            short: None,
            long: "start-at",
            arg: true,
        },
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
    let (given, operands) = getopt::parse(PROG, &opts, std::env::args().skip(1), false);
    let (mut out, mut start) = (None, None);
    for (o, value) in &given {
        let value = value.as_deref().unwrap_or_default();
        match *o {
            "help" => {
                print!("{}", usage());
                return;
            }
            "version" => {
                getopt::version(PROG);
                return;
            }
            "output" => out = Some(value),
            _ => {
                let cell = value.parse().ok().filter(|&c: &usize| c < 100 && value.len() <= 2);
                start = Some(cell.unwrap_or_else(|| {
                    getopt::usage_error(
                        PROG,
                        format!("invalid cell '{value}' for '--start-at': a cell is 00 to 99"),
                    )
                }));
            }
        }
    }
    match &operands[..] {
        [f] => carc(f, out, start),
        [] => getopt::usage_error(PROG, "no input file"),
        [_, extra, ..] => getopt::usage_error(PROG, format!("extra operand '{extra}'")),
    }
}
