//! Listings: a program as CARDIAC's own sources print it, so one copied out of the book just runs.
//!
//! One rule covers every format: **the first two numbers on a line are the address and the contents; everything
//! after them is optional.** A line that doesn't start with a number (a title, a heading, prose) is skipped. So
//! these all read:
//!
//! - the 1968 manual's program tables, `ADDRESS  CONTENTS  COMMENTS`, the comments in prose (curly quotes, ditto
//!   marks and all), and its look-up tables of two address and contents pairs a line;
//! - Brian Stuart's six columns, `Address | Contents | Label | Opcode | Operand | Comment`, `DATA` the one
//!   pseudo-op. Contents, when present, are what the cell holds; when missing, they are assembled from the opcode
//!   and the operand (a number or a label); when both are there and disagree, the contents win, with a warning;
//! - the manual's loading programs, `INPUT CARD  CONTENTS  COMMENTS` (Program No. 7): a deck, card by card, a
//!   `BLANK` card ending the program and data cards after it (`---`, a card still to be written, is left for the
//!   input). A listing is a deck when a heading before its first card names a CARD column.
//!
//! A cell whose contents the listing leaves to the reader (`8__`, `N(start)`) is left as it is, and said so
//! (`Read::unwritten`).
use std::collections::HashMap;

/// A program as listed.
#[derive(Clone, Debug, PartialEq)]
pub enum Listing {
    /// cells written straight into memory (the manual's short cut), in the order listed
    Memory(Vec<(usize, i32)>),
    /// cards for the reader
    Deck(Deck),
}

/// A deck as the manual lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Deck {
    /// the cards up to the `BLANK` card, or all of them
    pub cards: Vec<i32>,
    /// whether a `BLANK` card ends the program: the loader stops there, and the operator starts the program
    pub blank: bool,
    /// the data cards after the `BLANK` card, those written
    pub data: Vec<i32>,
}

/// A listing read, and what didn't look right in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Read {
    pub listing: Listing,
    pub warnings: Vec<String>,
    /// the cells whose contents the listing leaves to the reader (`8__`, `N(start)`): each cell, and what stands
    /// in for its contents
    pub unwritten: Vec<(usize, String)>,
    /// where the program starts, when the listing says: its first instruction, in a listing that names its opcodes
    pub start: Option<usize>,
}

const OPS: [&str; 10] = ["INP", "CLA", "ADD", "TAC", "SFT", "OUT", "STO", "SUB", "JMP", "HRS"];

/// A word as a listing writes it: an optional sign and digits, -999..999.
fn word(t: &str) -> Option<i32> {
    let d = t.strip_prefix(['+', '-']).unwrap_or(t);
    if d.is_empty() || !d.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    t.parse::<i32>().ok().filter(|v| (-999..=999).contains(v))
}

/// An address: digits, 0..99.
fn address(t: &str) -> Option<usize> {
    if t.is_empty() || !t.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    t.parse::<usize>().ok().filter(|&a| a < 100)
}

fn is_label(t: &str) -> bool {
    let mut c = t.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_')
        && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
        && !OPS.contains(&t)
        && t != "DATA"
}

/// The cards a line of a deck is made of: a card's contents, `BLANK`, or a data card still to be written.
enum Card {
    Word(i32),
    Blank,
    Unwritten,
}

fn card(t: &str) -> Option<Card> {
    if t == "BLANK" {
        return Some(Card::Blank);
    }
    if !t.is_empty() && t.chars().all(|c| matches!(c, '-' | '–' | '—' | '_')) {
        return Some(Card::Unwritten);
    }
    word(t).map(Card::Word)
}

/// A line of a memory listing, before its labels are known.
struct Line {
    n: usize,
    at: usize,
    contents: Option<i32>,
    /// what stands where the contents go when it isn't a number
    unwritten: Option<String>,
    /// the opcode (0-9, or 10 for DATA) and its operand, as written
    instruction: Option<(usize, String)>,
}

/// The tokens of a line: whitespace and `|` separate them.
fn tokens(line: &str) -> Vec<&str> {
    line.split(|c: char| c.is_whitespace() || c == '|')
        .filter(|t| !t.is_empty())
        .collect()
}

/// Read a listing.
pub fn parse(text: &str) -> Result<Read, String> {
    // a deck when a heading before the first card or cell names a CARD column
    let mut deck = false;
    for line in text.lines() {
        let t = tokens(line);
        if t.first().is_some_and(|f| address(f).is_some()) {
            break;
        }
        if t.iter().any(|w| w.eq_ignore_ascii_case("CARD")) {
            deck = true;
            break;
        }
    }
    if deck {
        parse_deck(text)
    } else {
        parse_memory(text)
    }
}

fn parse_deck(text: &str) -> Result<Read, String> {
    let mut d = Deck {
        cards: vec![],
        blank: false,
        data: vec![],
    };
    let mut warnings = vec![];
    for (n, line) in text.lines().enumerate() {
        let t = tokens(line);
        // a card: its number, then what is on it (a line with one number, such as the bootstrap already in cell
        // 00, is not a card)
        let (Some(no), Some(c)) = (t.first(), t.get(1)) else {
            continue;
        };
        if !no.bytes().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Some(c) = card(c) else {
            continue;
        };
        match c {
            Card::Word(w) if d.blank => d.data.push(w),
            Card::Word(w) => d.cards.push(w),
            Card::Blank if d.blank => warnings.push(format!("line {}: a second BLANK card, skipped", n + 1)),
            Card::Blank => d.blank = true,
            Card::Unwritten if d.blank => {}
            Card::Unwritten => {
                return Err(format!(
                    "line {}: card {no} is still to be written, and it is part of the program",
                    n + 1
                ))
            }
        }
    }
    if d.cards.is_empty() {
        return Err("no cards: a deck lists each card's number and contents".into());
    }
    Ok(Read {
        listing: Listing::Deck(d),
        warnings,
        unwritten: vec![],
        start: None,
    })
}

/// What a line of a memory listing says about its cell, past the address.
struct Cell {
    contents: Option<i32>,
    unwritten: Option<String>,
    label: Option<String>,
    instruction: Option<(usize, String)>,
}

/// An opcode, 0-9, or 10 for DATA.
fn op(t: &str) -> Option<usize> {
    OPS.iter().position(|&o| o == t).or((t == "DATA").then_some(10))
}

/// The contents column: a word, nothing, or something that stands for a word (`8__`, `N(start)`).
fn contents(c: &str) -> Result<(Option<i32>, Option<String>), String> {
    if c.is_empty() {
        return Ok((None, None));
    }
    if let Some(w) = word(c) {
        return Ok((Some(w), None));
    }
    if c.bytes().any(|b| b.is_ascii_digit()) && c.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-')) {
        return Err(format!("{c} is not a word (-999 to 999)"));
    }
    Ok((None, Some(c.to_string())))
}

/// A line in Stuart's columns, separated by tabs, an empty column where a cell has no label (or no contents):
/// `Address, Contents, Label, Opcode, Operand, Comment`. None when its columns aren't those.
fn columns(raw: &str) -> Option<Cell> {
    if !raw.contains('\t') {
        return None;
    }
    let c: Vec<&str> = raw.split('\t').map(|c| c.trim().trim_matches('|').trim()).collect();
    let col = |i: usize| c.get(i).copied().unwrap_or("");
    let label = col(2).trim_end_matches(':');
    if address(col(0)).is_none()
        || !(label.is_empty() || is_label(label))
        || !(col(3).is_empty() || op(col(3)).is_some())
    {
        return None;
    }
    let (contents, unwritten) = contents(col(1)).ok()?;
    Some(Cell {
        contents,
        unwritten,
        label: (!label.is_empty()).then(|| label.to_string()),
        // HRS with no operand is HRS 00
        instruction: op(col(3)).map(|o| (o, if col(4).is_empty() { "0" } else { col(4) }.to_string())),
    })
}

/// A line read word by word: the contents if the second word is one, then a label, an opcode and its operand,
/// each optional, and the rest comment.
fn guessed(t: &[&str]) -> Result<Cell, String> {
    let mut i = 1;
    let (contents, mut unwritten) = match t.get(1) {
        Some(c) => match contents(c)? {
            (Some(w), _) => {
                i = 2;
                (Some(w), None)
            }
            // not a word: perhaps a label or an opcode, else it stands for the contents
            _ => (None, None),
        },
        None => (None, None),
    };
    let mut label = None;
    let mut instruction = None;
    match (t.get(i), t.get(i + 1)) {
        // (after the contents, `word:` is prose: `data: CLA 97: an instruction the program changes`)
        (Some(&l), Some(&o))
            if is_label(l.trim_end_matches(':')) && op(o).is_some() && !(contents.is_some() && l.ends_with(':')) =>
        {
            label = Some(l.trim_end_matches(':').to_string());
            instruction = t.get(i + 2).map(|x| (op(o).unwrap(), x.to_string()));
            if instruction.is_none() && contents.is_none() {
                return Err(format!("{o} with no operand"));
            }
        }
        (Some(&o), x) if op(o).is_some() => {
            instruction = x.map(|x| (op(o).unwrap(), x.to_string()));
            if instruction.is_none() && contents.is_none() {
                return Err(format!("{o} with no operand"));
            }
        }
        (Some(&c), _) if contents.is_none() => unwritten = Some(c.to_string()),
        _ => {}
    }
    Ok(Cell {
        contents,
        unwritten,
        label,
        instruction,
    })
}

fn parse_memory(text: &str) -> Result<Read, String> {
    let mut lines: Vec<Line> = vec![];
    let mut labels: HashMap<String, usize> = HashMap::new();
    let mut warnings = vec![];
    // a label on a line of its own (`S_Loop:`) names the next cell listed
    let mut pending: Option<(usize, String)> = None;
    let define = |l: &str, at: usize, n: usize, labels: &mut HashMap<String, usize>| {
        if labels.insert(l.to_string(), at).is_some() {
            return Err(format!("line {}: label {l} defined twice", n + 1));
        }
        Ok(())
    };
    for (n, raw) in text.lines().enumerate() {
        let t = tokens(raw);
        let Some(at) = t.first().and_then(|f| address(f)) else {
            if let [l] = t.as_slice() {
                if let Some(l) = l.strip_suffix(':').filter(|l| is_label(l)) {
                    pending = Some((n, l.to_string()));
                }
            }
            continue;
        };
        // a table of pairs, two or more a line: every token a number
        if t.len() >= 4
            && t.len().is_multiple_of(2)
            && t.chunks(2).all(|p| address(p[0]).is_some() && word(p[1]).is_some())
        {
            for p in t.chunks(2) {
                lines.push(Line {
                    n,
                    at: address(p[0]).unwrap(),
                    contents: word(p[1]),
                    unwritten: None,
                    instruction: None,
                });
            }
            continue;
        }
        let cell = columns(raw).map_or_else(|| guessed(&t), Ok);
        let Cell {
            contents,
            unwritten,
            label,
            instruction,
        } = cell.map_err(|e| format!("line {}: cell {at:02}: {e}", n + 1))?;
        if let Some((ln, l)) = pending.take() {
            define(&l, at, ln, &mut labels)?;
        }
        if let Some(l) = &label {
            define(l, at, n, &mut labels)?;
        }
        if contents.is_none() && instruction.is_none() && unwritten.is_none() {
            // an address alone, or with a label only: nothing to load
            continue;
        }
        lines.push(Line {
            n,
            at,
            contents,
            unwritten,
            instruction,
        });
    }
    // where it starts: the first instruction, when the listing names its opcodes (Stuart's six columns: the data
    // cells come first)
    let start = lines
        .iter()
        .find(|l| matches!(l.instruction, Some((o, _)) if o < 10))
        .map(|l| l.at);
    let operand = |x: &str| -> Option<i32> {
        if let Some(&a) = labels.get(x) {
            return Some(a as i32);
        }
        word(x)
    };
    let mut cells: Vec<(usize, i32)> = vec![];
    let mut unwritten = vec![];
    for l in &lines {
        let assembled = match &l.instruction {
            Some((10, x)) => operand(x),
            Some((o, x)) => operand(x).filter(|a| (0..100).contains(a)).map(|a| *o as i32 * 100 + a),
            None => None,
        };
        let value = match (l.contents, assembled, &l.instruction) {
            (Some(c), Some(a), Some((o, x))) if c != a => {
                let name = if *o == 10 { "DATA" } else { OPS[*o] };
                warnings.push(format!(
                    "line {}: cell {:02} holds {c:03}, and {name} {x} is {a:03}: taking {c:03}",
                    l.n + 1,
                    l.at
                ));
                c
            }
            (Some(c), _, _) => c,
            (None, Some(a), _) => a,
            (None, None, Some((o, x))) => {
                let name = if *o == 10 { "DATA" } else { OPS[*o] };
                return Err(format!(
                    "line {}: cell {:02}: {name} {x}: unknown label or bad operand",
                    l.n + 1,
                    l.at
                ));
            }
            (None, None, None) => {
                unwritten.push((l.at, l.unwritten.clone().unwrap_or_default()));
                continue;
            }
        };
        if l.at == 0 && value != 1 {
            warnings.push(format!(
                "line {}: cell 00 is wired to 001: {value:03} not loaded",
                l.n + 1
            ));
        }
        if let Some(c) = cells.iter_mut().find(|c| c.0 == l.at) {
            warnings.push(format!(
                "line {}: cell {:02} listed again: taking {value:03}",
                l.n + 1,
                l.at
            ));
            c.1 = value;
        } else {
            cells.push((l.at, value));
        }
    }
    if cells.is_empty() {
        return Err("no cells: a listing gives each cell's address and contents".into());
    }
    Ok(Read {
        listing: Listing::Memory(cells),
        warnings,
        unwritten,
        start,
    })
}

/// Memory after loading `cells` into a cleared CARDIAC (cell 00 stays wired to 001).
pub fn memory(cells: &[(usize, i32)]) -> [i32; 100] {
    let mut m = [0; 100];
    for &(a, v) in cells {
        m[a] = v;
    }
    m[0] = 1;
    m
}

/// A self-loading deck that puts `cells` into memory and starts at `start`, laid out as Brian Stuart's page has it:
/// the bootstrap cards 002 and 800, an address card and a contents card for each cell, then the pair 002 8xx, which
/// puts a jump to the start into the loader's cell 02. The loader runs in cells 01 and 02, and each of its jumps
/// writes cell 99, so a deck can't load those three.
pub fn deck(start: usize, cells: &[(usize, i32)]) -> Result<Vec<i32>, String> {
    if let Some((a, _)) = cells.iter().find(|(a, _)| [1, 2, 99].contains(a)) {
        return Err(format!(
            "cell {a:02} can't be loaded from cards: the loader uses cells 01 and 02, and its jumps write 99"
        ));
    }
    let mut d = vec![2, 800];
    for &(a, v) in cells.iter().filter(|(a, _)| *a != 0) {
        d.push(a as i32);
        d.push(v);
    }
    d.extend([2, 800 + start as i32]);
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(text: &str) -> Vec<(usize, i32)> {
        match parse(text).unwrap().listing {
            Listing::Memory(c) => c,
            Listing::Deck(_) => panic!("a deck"),
        }
    }

    #[test]
    fn the_manuals_table_reads() {
        let c = cells(
            "Program No. 6\n\nADDRESS  CONTENTS  COMMENTS\n  15       039     Read \u{201c}abc\u{201d} into cell 39.\n  16  139  CLA \"abc\".\n  24   +321  Test acc. { If minus\n            { If plus, go ahead to cell 25.\n",
        );
        assert_eq!(c, vec![(15, 39), (16, 139), (24, 321)]);
    }

    #[test]
    fn stuarts_six_columns_assemble_what_is_missing() {
        let r = parse(
            "Address | Contents | Label | Opcode | Operand | Comment\n\
             --- | --- | --- | --- | --- | ---\n\
             10 | 034 | | INP | a | read a\n\
             11 | | top | CLA | a |\n\
             12 | | | OUT | a | print it\n\
             13 | 899 | | JMP | top | wrong contents\n\
             34 | | a | DATA | -5 |\n",
        )
        .unwrap();
        assert_eq!(
            r.listing,
            Listing::Memory(vec![(10, 34), (11, 134), (12, 534), (13, 899), (34, -5)])
        );
        assert_eq!(r.warnings.len(), 1);
        assert!(r.warnings[0].contains("JMP top is 811"), "{:?}", r.warnings);
        // without the bars, too
        assert_eq!(
            cells("11  top CLA a\n12 OUT a\n34 a DATA 7\n"),
            vec![(11, 134), (12, 534), (34, 7)]
        );
        assert!(parse("11 CLA nowhere\n").is_err());
    }

    #[test]
    fn stuarts_tab_columns_labels_alone_and_the_start() {
        // data first, a label with no contents (a variable), a label on a line of its own, HRS with no operand
        let r = parse(
            "05\t600\tstorer\tDATA\t600\n06\t\tn\t\t\tthe count\n\n\t\ttop:\n10\t\t\tCLA\tn\tload it\n11\t\t\tJMP\ttop\n12\t900\t\tHRS\n",
        )
        .unwrap();
        assert_eq!(
            r.listing,
            Listing::Memory(vec![(5, 600), (10, 106), (11, 810), (12, 900)])
        );
        assert_eq!((r.start, r.warnings.len(), r.unwritten.len()), (Some(10), 0, 0));
        // a listing with no opcodes says nothing of where it starts
        assert_eq!(parse("17 034\n18 035\n").unwrap().start, None);
    }

    #[test]
    fn prose_after_the_contents_is_prose() {
        // `data:` in a note is not a label, though an opcode follows it
        let c = cells(" 86     197      data: CLA 97: an instruction the program copies\n 88  097  data: INP 97\n");
        assert_eq!(c, vec![(86, 197), (88, 97)]);
    }

    #[test]
    fn a_table_of_pairs_and_cells_left_to_the_reader() {
        let r = parse("ADDRESS CONTENTS ADDRESS CONTENTS\n 10 000 24 013\n 11 001 25 014\n 94 8__ } Return\n").unwrap();
        assert_eq!(r.listing, Listing::Memory(vec![(10, 0), (24, 13), (11, 1), (25, 14)]));
        assert_eq!(r.unwritten, vec![(94, "8__".to_string())]);
        assert!(parse("10 1000\n").is_err());
    }

    #[test]
    fn a_deck_with_a_blank_card() {
        let r = parse(
            "INPUT CARD  CONTENTS  COMMENTS\n      001  Bootstrap\n 1  002  \"\n 2  800\n 3  010\n 4  900\n 5  BLANK  end\n 6  ---  Data\n 7  042\n",
        )
        .unwrap();
        assert_eq!(
            r.listing,
            Listing::Deck(Deck {
                cards: vec![2, 800, 10, 900],
                blank: true,
                data: vec![42],
            })
        );
    }

    #[test]
    fn a_deck_starts_at_its_start() {
        assert_eq!(deck(17, &[(0, 1), (17, 34)]).unwrap(), vec![2, 800, 17, 34, 2, 817]);
        assert!(deck(3, &[(2, 900)]).is_err());
    }
}
