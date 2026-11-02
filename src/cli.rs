//! What burn, carc and ekg share on the command line besides getopt: what is wrong in a listing, compiler style; a
//! deck told apart from a listing by what is in it; and standard input as the card reader. Each program uses some
//! of it.
#![allow(dead_code)]

use std::io::BufRead;
use std::path::Path;

use heartburn::assemble::{self, Assembly};
use heartburn::tape::{self, values, Tape};

/// What is wrong in a listing, a line each on standard error in the order of its lines, `PREFIX FILE:LINE:COLUMN:
/// error: WHAT`: the assembler's diagnostics and, with `loader`, what the deck's loader does to its cells (for a
/// listing run as its deck). Whether any of it is an error.
pub fn report(prefix: &str, file: &str, text: &str, asm: &Assembly, loader: bool) -> bool {
    let lines = assemble::lines(text);
    let extra = if loader {
        assemble::loader_warnings(text, asm)
    } else {
        vec![]
    };
    let mut all: Vec<_> = asm.diagnostics.iter().chain(&extra).collect();
    all.sort_by_key(|d| d.line);
    for d in all {
        let line = lines.get(d.line).copied().unwrap_or_default();
        let col = line.get(..d.span.start).unwrap_or_default().chars().count() + 1;
        let kind = if d.warning { "warning" } else { "error" };
        eprintln!("{prefix}{file}:{}:{col}: {kind}: {}", d.line + 1, d.message);
    }
    asm.diagnostics.iter().any(|d| !d.warning)
}

/// A warning when a listing's deck starts the program on a cell it doesn't load: one the listing leaves empty
/// (000, INP 00), or one of the loader's.
pub fn start_warning(start: usize, asm: &Assembly) -> Option<String> {
    if start <= 2 {
        Some(format!("the program starts at cell {start:02}, one of the loader's"))
    } else if !asm.rows.iter().any(|r| r.addr == start) {
        Some(format!(
            "the program starts at cell {start:02}, which the listing leaves empty (000, INP 00)"
        ))
    } else {
        None
    }
}

/// What a file is, by what is in it: a deck (`Some`), or not one (`None`: a listing, say). Text meant for a deck
/// that doesn't read as one (a card too big, cards that don't start with the loader) is an error, said as such,
/// not taken for something else. `path` is the file, to read the decks of a program in phases beside it; none for
/// standard input.
pub fn deck_or_not(name: &str, text: &str, path: Option<&Path>) -> Result<Option<Tape>, String> {
    if !tape::is_deck_text(text) {
        return Ok(None);
    }
    let deck = match path {
        Some(p) => Tape::load(p)?,
        None => Tape::parse(text).map_err(|e| format!("{name}: {e}"))?,
    };
    if !deck.phases.first().is_some_and(|cards| cards.starts_with(&[2, 800])) {
        return Err(format!(
            "{name}: not a CARDIAC deck (a deck starts with the loader's cards, 002 and 800)"
        ));
    }
    Ok(Some(deck))
}

/// Standard input as the card reader: each number a card, read when the program asks for one. What isn't a card
/// ends the program `prog`.
pub fn stdin_cards(prog: &'static str) -> impl Iterator<Item = i32> {
    let mut lines = std::io::stdin().lock().lines();
    let mut pending: std::vec::IntoIter<i32> = vec![].into_iter();
    std::iter::from_fn(move || loop {
        if let Some(c) = pending.next() {
            return Some(c);
        }
        let line = lines
            .next()?
            .unwrap_or_else(|e| crate::getopt::die(prog, format!("standard input: {}", crate::getopt::os_error(&e))));
        pending = values(&line)
            .unwrap_or_else(|e| crate::getopt::die(prog, format!("standard input: {e}")))
            .into_iter();
    })
}
