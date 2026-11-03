//! carc: assemble a CARDIAC listing into a deck. `burn` runs decks; carc makes them, from a listing in Brian
//! Stuart's six columns, with the assembler Cardigan uses (`heartburn::assemble`), so a listing assembles the same
//! here and in the IDE.
use std::io::{Read, Write};
use std::path::Path;

use heartburn::assemble;

#[path = "../cli.rs"]
mod cli;
#[path = "../getopt.rs"]
mod getopt;

const PROG: &str = "carc";

fn usage() -> &'static str {
    "\
Usage: carc [OPTION]... FILE
Assemble a CARDIAC listing into a deck for burn.

A listing has one cell per line. The columns are either Brian Stuart's six, ADDRESS CONTENTS LABEL OPCODE
OPERAND COMMENT, or the manual's three, ADDRESS CONTENTS COMMENT. You can leave the contents out, and carc works
them out from the opcode and the operand. Lines that aren't cells, such as titles, prose, and comments starting
with ; or #, are skipped. When FILE is -, the listing is read from standard input.

The deck starts with the loader's cards, 002 and 800. Then come two cards for each cell, its address and its
contents, and finally 002 and 8NN, which start the program at cell NN. NN is the cell given by --start-at.
Without it, carc looks in the listing: first for a \"Start at NN.\" line, then for a line labelled start, and
otherwise it uses the first instruction.

  -o, --output=FILE  write the deck to FILE (- means standard output). By default it goes to NAME.cqb
                       next to NAME.cq, or to standard output if the listing came from standard input.
      --start-at=NN  start the program at cell NN
  -h, --help         display this help and exit
      --version      output version information and exit

Problems are reported as FILE:LINE:COLUMN: error: WHAT, or as warning: WHAT. A line with an error isn't
loaded, and no deck is written while there are errors.

The exit status is 0 if the deck was written, 1 if the listing has errors or a file can't be read or written,
and 2 if there is a mistake on the command line.
"
}

fn die(why: impl std::fmt::Display) -> ! {
    getopt::die(PROG, why)
}

/// Whether two paths are the same file (one that doesn't exist yet is no file at all).
fn same_file(a: &str, b: &str) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
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
    // with what the loader does to cells 01, 02 and 99: the deck is what runs
    let errors = cli::report("", listing, &text, &asm, true);
    if errors {
        die(format!("{listing}: not assembled: no deck written"))
    }
    let Some(start) = start.or(asm.start) else {
        die(format!("{listing}: no cells: nothing to put on a deck"))
    };
    let deck = match out {
        Some(o) => o.to_string(),
        None if file == "-" => "-".to_string(),
        None => Path::new(file).with_extension("cqb").to_string_lossy().into_owned(),
    };
    // never the listing written over with its deck (a listing called NAME.cqb, or -o naming it)
    if file != "-" && deck != "-" && same_file(file, &deck) {
        die(format!(
            "{deck}: that is the listing itself: write the deck to another file with -o"
        ))
    }
    if let Some(w) = cli::start_warning(start, &asm) {
        eprintln!("{listing}: warning: {w}");
    }
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
