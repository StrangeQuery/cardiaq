//! The assembler: a listing in Brian Stuart's six columns made into the cells it describes, and a deck for the
//! loader. It is what `carc` runs, and what Cardigan, the CARDIAC IDE, reads its listings with (through
//! heartburn.wasm).
//!
//! A listing is text. A line is a cell when it starts with an address and then the contents (or an opcode, the
//! contents then worked out from it): `ADDRESS CONTENTS [LABEL] [OPCODE OPERAND] [COMMENT]`, the six columns of
//! Brian Stuart's page, or the manual's `ADDRESS CONTENTS COMMENT`. An opcode is one of the ten in capitals, or
//! `DATA` (`DAT`), followed by an address, a number or a label (`label` or `label+N`); anything else after the
//! contents is a comment (`CLA "abc"` in the manual is prose). Every other line (a title, headings, prose, `;` or
//! `#` comments, blank lines) is kept and not loaded.
//!
//! Assembling never fails: a line it cannot load is flagged, with what is wrong and where, and the rest is loaded.
//! The address and contents are what is loaded; an opcode and operand that say otherwise are a warning, and the
//! contents load.
//!
//! Spans are byte offsets into their line. A web page counts in UTF-16 units: `utf16` converts.
use std::collections::BTreeMap;
use std::ops::Range;

/// The ten opcodes, in order: an opcode's index is its digit.
pub const OPS: [&str; 10] = ["INP", "CLA", "ADD", "TAC", "SFT", "OUT", "STO", "SUB", "JMP", "HRS"];

/// What a line is.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Blank,
    /// a `;` or `#` comment, from this offset to the end of the line
    Comment(usize),
    /// anything else that isn't a cell (a title, a heading, prose), from this offset to the end of the line
    Prose(usize),
    Cell(Cell),
}

/// A cell's line, its columns: where each is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cell {
    pub address: Range<usize>,
    pub contents: Option<Range<usize>>,
    pub label: Option<Range<usize>>,
    pub opcode: Option<Range<usize>>,
    pub operand: Option<Range<usize>>,
    /// where the comment starts; it runs to the end of the line
    pub comment: Option<usize>,
}

/// A cell the listing loads.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// its line, from 0
    pub line: usize,
    pub addr: usize,
    pub contents: i32,
    /// its label, without the colon
    pub label: Option<String>,
    /// its opcode, `DAT` written `DATA`
    pub op: Option<&'static str>,
    /// its operand as written (`00` when an opcode has none)
    pub operand: Option<String>,
    pub comment: String,
}

/// What is wrong on a line: one a line, the first found.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    /// the line, from 0
    pub line: usize,
    /// what it is about, in the line
    pub span: Range<usize>,
    pub message: String,
    /// a warning: the line is loaded all the same
    pub warning: bool,
    /// the cell is on an earlier line too
    pub duplicate: bool,
    /// on a warning that the contents disagree with the opcode and operand: what they make
    pub want: Option<i32>,
}

/// A listing assembled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Assembly {
    /// how many lines the listing has (blank lines at its end not counted)
    pub lines: usize,
    /// the cells it loads, in the order listed
    pub rows: Vec<Row>,
    /// in the order of their lines
    pub diagnostics: Vec<Diagnostic>,
    /// where it starts (see `assemble`); none for a listing with no cells
    pub start: Option<usize>,
    /// for each line: what its opcode and operand make, when they make something
    pub words: Vec<Option<i32>>,
}

/// The whitespace a web page's text means (JavaScript's `\s`), so a line splits the same way in both.
fn space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

fn trim(s: &str) -> &str {
    s.trim_matches(space)
}

/// The words of a line, and where each is.
fn words(line: &str) -> Vec<(Range<usize>, &str)> {
    let mut w = vec![];
    let mut from = None;
    for (i, c) in line.char_indices().chain([(line.len(), ' ')]) {
        match (space(c), from) {
            (true, Some(s)) => {
                w.push((s..i, &line[s..i]));
                from = None
            }
            (false, None) => from = Some(i),
            _ => {}
        }
    }
    w
}

fn digits(t: &str) -> bool {
    !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit())
}

/// A number: digits, a sign allowed.
fn is_num(t: &str) -> bool {
    digits(t.strip_prefix(['+', '-']).unwrap_or(t))
}

/// A number's value; one too big for a cell is big enough to say so, however long.
fn value(t: &str) -> i64 {
    let d = t.strip_prefix(['+', '-']).unwrap_or(t);
    let v = d.bytes().fold(0i64, |v, b| (v * 10 + (b - b'0') as i64).min(1 << 50));
    if t.starts_with('-') {
        -v
    } else {
        v
    }
}

fn is_op(t: &str) -> bool {
    OPS.contains(&t) || t == "DATA" || t == "DAT"
}

fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// A name: a letter or `_`, then letters, digits and `_`.
fn is_name(t: &str) -> bool {
    t.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') && t.chars().all(word_char)
}

fn is_label(t: &str) -> bool {
    is_name(t.strip_suffix(':').unwrap_or(t))
}

/// `label` and `label+N`: the name, and N.
fn label_ref(t: &str) -> Option<(&str, &str)> {
    match t.split_once('+') {
        Some((name, n)) if digits(n) => Some((name, n)),
        Some(_) => None,
        None => Some((t, "")),
    }
    .filter(|(name, _)| is_name(name))
}

fn is_operand(t: &str) -> bool {
    is_num(t) || label_ref(t).is_some()
}

/// What a line is, and where its columns are.
pub fn tokenize(line: &str) -> Line {
    let w = words(line);
    let Some((first, t0)) = w.first() else {
        return Line::Blank;
    };
    if t0.starts_with([';', '#']) {
        return Line::Comment(first.start);
    }
    let cellish = is_num(t0)
        && w.get(1)
            .is_some_and(|(_, t1)| is_num(t1) || is_op(t1) || (w.len() > 2 && is_label(t1) && is_op(w[2].1)));
    if !cellish {
        return Line::Prose(first.start);
    }
    let mut c = Cell {
        address: first.clone(),
        ..Cell::default()
    };
    let mut k = 1;
    if is_num(w[k].1) {
        c.contents = Some(w[k].0.clone());
        k += 1;
    }
    // an opcode is one when what follows it is an operand, or nothing, or the contents column is empty
    let no_contents = c.contents.is_none();
    let op_fits = |j: usize| {
        w.get(j).is_some_and(|(_, t)| is_op(t)) && w.get(j + 1).is_none_or(|(_, t)| is_operand(t) || no_contents)
    };
    if w.get(k).is_some_and(|(_, t)| !is_op(t) && is_label(t)) && op_fits(k + 1) {
        c.label = Some(w[k].0.clone());
        k += 1;
    }
    if op_fits(k) {
        c.opcode = Some(w[k].0.clone());
        k += 1;
        if w.get(k).is_some_and(|(_, t)| is_operand(t) || no_contents) {
            c.operand = Some(w[k].0.clone());
            k += 1;
        }
    }
    c.comment = w.get(k).map(|(r, _)| r.start);
    Line::Cell(c)
}

/// A listing's lines: split at line breaks (`\n` or `\r\n`), blank lines at the end dropped.
pub fn lines(text: &str) -> Vec<&str> {
    let mut l: Vec<&str> = text.split('\n').collect();
    let last = l.len() - 1;
    for s in &mut l[..last] {
        *s = s.strip_suffix('\r').unwrap_or(s);
    }
    while l.len() > 1 && trim(l[l.len() - 1]).is_empty() {
        l.pop();
    }
    l
}

/// A byte offset into a line, in UTF-16 units.
pub fn utf16(line: &str, byte: usize) -> usize {
    line[..byte].encode_utf16().count()
}

/// A word as the page shows it: a sign, always, and three digits.
fn word3(v: i64) -> String {
    format!("{}{:03}", if v < 0 { '\u{2212}' } else { '+' }, v.abs())
}

/// A word as a listing or a deck writes it: three digits, and `-` if negative.
pub fn written(v: i32) -> String {
    format!("{}{:03}", if v < 0 { "-" } else { "" }, v.abs())
}

/// An operand's value: a number, or a label's cell (plus N); none for a label not defined.
fn resolve(operand: &str, labels: &BTreeMap<&str, i64>) -> Option<i64> {
    if is_num(operand) {
        return Some(value(operand));
    }
    let (name, n) = label_ref(operand)?;
    Some(labels.get(name)? + if n.is_empty() { 0 } else { value(n) })
}

/// Assemble a listing: its cells (those without errors), what is wrong where, and where it starts. Never fails: a
/// line it cannot load is flagged, and the rest is loaded.
///
/// Labels are collected first (the first definition of a name is the one), so a line may name a label defined
/// below it. A line with an error isn't loaded. Cell 00 is wired to 001: a line giving it anything else isn't
/// loaded. A line that starts with a cell's address and names an opcode but isn't a cell (a word typed into it,
/// say) is flagged, not taken as prose; a manual's table row (`14  N(start)  N = number of pebbles`) names none and
/// stays prose.
///
/// Where it starts: a line that says so (`Start at NN.`, or `Start ... cell NN`: the first word is "start", and
/// the cell follows "at" or "cell"), else its first instruction (the line labelled `start`, or the first cell whose
/// opcode isn't DATA), else the first cell listed.
pub fn assemble(text: &str) -> Assembly {
    let lines = lines(text);
    let info: Vec<Line> = lines.iter().map(|l| tokenize(l)).collect();
    let cells = || {
        info.iter()
            .enumerate()
            .filter_map(|(n, l)| if let Line::Cell(c) = l { Some((n, c)) } else { None })
    };
    let mut labels: BTreeMap<&str, i64> = BTreeMap::new();
    for (n, c) in cells() {
        let addr = &lines[n][c.address.clone()];
        if let (Some(lb), true) = (&c.label, digits(addr)) {
            let l = &lines[n][lb.clone()];
            labels.entry(l.strip_suffix(':').unwrap_or(l)).or_insert(value(addr));
        }
    }
    let mut asm = Assembly {
        lines: lines.len(),
        words: vec![None; lines.len()],
        ..Assembly::default()
    };
    let mut seen_addr: [Option<usize>; 100] = [None; 100];
    let mut seen_label: BTreeMap<&str, usize> = BTreeMap::new();
    for (ln, l) in info.iter().enumerate() {
        let line = lines[ln];
        let Line::Cell(c) = l else {
            asm.diagnostics.extend(stray(ln, line));
            continue;
        };
        let tok = |r: &Option<Range<usize>>| r.as_ref().map(|r| &line[r.clone()]);
        // what is wrong with the line: the first thing found
        let mut flag: Option<Diagnostic> = None;
        let mut raise = |msg: String, span: Option<&Range<usize>>| {
            flag.get_or_insert_with(|| error(ln, span.cloned().unwrap_or(0..line.len()), msg));
        };
        let addr_t = &line[c.address.clone()];
        let a = value(addr_t);
        let mut duplicate = false;
        if !digits(addr_t) || a > 99 {
            raise("the address is not a cell 00-99".into(), Some(&c.address));
        } else if let Some(first) = seen_addr[a as usize] {
            raise(format!("cell {a:02} is on line {} too", first + 1), Some(&c.address));
            duplicate = true;
        } else {
            seen_addr[a as usize] = Some(ln);
        }
        let label = tok(&c.label).map(|l| l.strip_suffix(':').unwrap_or(l));
        if let Some(l) = label {
            if seen_label.contains_key(l) {
                raise(format!("label {l} is defined twice"), c.label.as_ref());
            } else {
                seen_label.insert(l, ln);
            }
        }
        let op = tok(&c.opcode).map(|o| OPS.into_iter().find(|&p| p == o).unwrap_or("DATA"));
        let operand = match (tok(&c.operand), op) {
            (Some(t), _) => Some(t),
            (None, Some(o)) if o != "DATA" => Some("00"),
            _ => None,
        };
        let mut want = None;
        if let Some(o) = op {
            match operand.map(|arg| (arg, resolve(arg, &labels))) {
                None => raise("DATA with no value".into(), c.opcode.as_ref()),
                Some((arg, None)) => raise(format!("unknown label {arg}"), c.operand.as_ref()),
                Some((arg, Some(v))) if o != "DATA" && !(0..=99).contains(&v) => {
                    raise(format!("operand {arg} is out of 00-99"), c.operand.as_ref())
                }
                Some((_, Some(v))) if o == "DATA" && v.abs() > 999 => {
                    raise("DATA out of -999..999".into(), c.operand.as_ref())
                }
                Some((_, Some(v))) => {
                    want = Some(match OPS.iter().position(|&p| p == o) {
                        Some(digit) => digit as i64 * 100 + v,
                        None => v,
                    })
                }
            }
        }
        asm.words[ln] = want.map(|w| w as i32);
        let mut contents = want;
        if let Some(wd) = tok(&c.contents) {
            let v = value(wd);
            contents = Some(v);
            if v.abs() > 999 {
                raise("contents out of -999..999".into(), c.contents.as_ref());
            } else if wd.trim_start_matches(['+', '-']).len() != 3 {
                // contents are three digits, in every listing and the manual: a short one is most likely a typo
                raise(
                    format!("the contents is three digits ({})", word3(v)),
                    c.contents.as_ref(),
                );
            }
        }
        if let Some(mut f) = flag {
            // a line with an error is not loaded
            f.duplicate = duplicate;
            asm.diagnostics.push(f);
            continue;
        }
        // a cell always has contents or an opcode, and an opcode without an error makes some
        let contents = contents.unwrap_or_default();
        if a == 0 && contents != 1 {
            let span = c.contents.clone().unwrap_or(c.address.clone());
            asm.diagnostics
                .push(error(ln, span, "cell 00 is wired to 001: not loaded".into()));
            continue;
        }
        if let (Some(w), Some(o), Some(arg), Some(span)) = (want, op, operand, &c.contents) {
            if contents != w {
                let msg = format!(
                    "contents {}, but {o} {arg} is {}: loading the contents",
                    word3(contents),
                    word3(w)
                );
                asm.diagnostics.push(Diagnostic {
                    warning: true,
                    want: Some(w as i32),
                    ..error(ln, span.clone(), msg)
                });
            }
        }
        asm.rows.push(Row {
            line: ln,
            addr: a as usize,
            contents: contents as i32,
            label: label.map(String::from),
            op,
            operand: op.and(operand).map(String::from),
            comment: c.comment.map_or(String::new(), |s| trim(&line[s..]).to_string()),
        });
    }
    asm.start = start(&lines, &info, &asm.rows);
    asm
}

/// A line that starts with a cell's address and names an opcode, but isn't a cell (a cell's line is never one).
fn stray(ln: usize, line: &str) -> Option<Diagnostic> {
    if !line.split(|c: char| !word_char(c)).any(is_op) {
        return None;
    }
    let lead = line.len() - line.trim_start_matches(space).len();
    let rest = &line[lead..];
    let n = rest.bytes().take_while(u8::is_ascii_digit).count();
    let after = &rest[n..];
    let gap = after.len() - after.trim_start_matches(space).len();
    if !(1..=2).contains(&n) || gap == 0 || gap == after.len() {
        return None;
    }
    let msg = format!(
        "cell {:02}: not a line of a listing (address, contents, label, opcode, operand, comment): not loaded",
        value(&rest[..n])
    );
    Some(error(ln, lead..line.len(), msg))
}

fn error(line: usize, span: Range<usize>, message: String) -> Diagnostic {
    Diagnostic {
        line,
        span,
        message,
        warning: false,
        duplicate: false,
        want: None,
    }
}

/// Where a listing starts (see `assemble`).
fn start(lines: &[&str], info: &[Line], rows: &[Row]) -> Option<usize> {
    for (raw, l) in lines.iter().zip(info) {
        if matches!(l, Line::Cell(_)) {
            continue;
        }
        let t: Vec<&str> = raw.split(|c| space(c) || c == '|').filter(|t| !t.is_empty()).collect();
        if t.first().is_none_or(|w| !w.eq_ignore_ascii_case("start")) {
            continue;
        }
        if let Some(k) = (1..t.len().saturating_sub(1))
            .find(|&k| t[k].eq_ignore_ascii_case("at") || t[k].eq_ignore_ascii_case("cell"))
        {
            let a = t[k + 1].trim_end_matches(['.', ',', ';', ')']);
            if digits(a) && value(a) < 100 {
                return Some(value(a) as usize);
            }
        }
    }
    rows.iter()
        .find(|r| r.label.as_deref() == Some("start"))
        .or_else(|| rows.iter().find(|r| r.op.is_some_and(|o| o != "DATA")))
        .or(rows.first())
        .map(|r| r.addr)
}

/// The deck the loader reads, the listing on cards: 002 800, then an address card and a contents card for each
/// cell listed after the loader's (03 on), in the order listed, then 002 8NN to start the program at `start`.
pub fn deck(asm: &Assembly, start: usize) -> Vec<i32> {
    let mut d = vec![2, 800];
    for r in asm.rows.iter().filter(|r| r.addr > 2) {
        d.extend([r.addr as i32, r.contents]);
    }
    d.extend([2, 800 + start as i32]);
    d
}

/// A deck as text: one card a line, three digits each (`-` if negative).
pub fn deck_text(cards: &[i32]) -> String {
    cards.iter().map(|&c| written(c) + "\n").collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows, compactly: `line:addr=contents`, then the label, opcode and operand, and the comment.
    fn rows(a: &Assembly) -> Vec<String> {
        a.rows
            .iter()
            .map(|r| {
                let mut s = format!("{}:{}={}", r.line, r.addr, r.contents);
                if let Some(l) = &r.label {
                    s += &format!(" {l}");
                }
                if let (Some(o), Some(arg)) = (r.op, &r.operand) {
                    s += &format!(" {o} {arg}");
                }
                if !r.comment.is_empty() {
                    s += &format!(" ;{}", r.comment);
                }
                s
            })
            .collect()
    }

    /// The diagnostics: `line [start,end] message`, `W` for a warning, `D` for a cell on an earlier line too.
    fn diagnostics(a: &Assembly) -> Vec<String> {
        a.diagnostics
            .iter()
            .map(|d| {
                let w = if d.warning { " W" } else { "" };
                let dup = if d.duplicate { " D" } else { "" };
                format!("{} [{},{}]{w}{dup} {}", d.line, d.span.start, d.span.end, d.message)
            })
            .collect()
    }

    #[test]
    fn a_listing_assembles() {
        let a = assemble(LISTING);
        assert_eq!(
            rows(&a),
            [
                "3:10=122 start CLA tab+2 ;the third",
                "4:11=520 OUT tab+0",
                "5:12=900 HRS 00",
                "6:20=1 tab DATA 1",
                "7:21=2 DATA 2",
                "8:22=3 DATA 3"
            ]
        );
        assert!(a.diagnostics.is_empty());
        assert_eq!((a.lines, a.start), (9, Some(10)));
        assert_eq!(a.words[3..6], [Some(122), Some(520), Some(900)]);
        assert_eq!(a.words[..3], [None, None, None]);
    }

    const LISTING: &str = "; tab+2\n\
                           ; Address Contents Label Opcode Operand Comment\n\
                           \n\
                           10       start  CLA  tab+2  the third\n\
                           11              OUT  tab+0\n\
                           12              HRS  00\n\
                           20       tab    DATA 1\n\
                           21              DATA 2\n\
                           22              DATA 3\n\
                           \n\
                           \n";

    #[test]
    fn a_deck_is_the_loader_the_cells_and_the_start() {
        let a = assemble(LISTING);
        assert_eq!(
            deck(&a, 10),
            [2, 800, 10, 122, 11, 520, 12, 900, 20, 1, 21, 2, 22, 3, 2, 810]
        );
        assert_eq!(deck_text(&[2, 800, 41, -52, 2, 803]), "002\n800\n041\n-052\n002\n803\n");
        // the loader's own cells aren't on it
        let a = assemble("00 001\n01 002\n03 900\n");
        assert_eq!(deck(&a, 3), [2, 800, 3, 900, 2, 803]);
    }

    #[test]
    fn a_cell_listed_twice_is_flagged_and_not_loaded() {
        let a = assemble("10 100 CLA 00\n11 200 ADD 00\n10 300 SFT 00\n11 400\n");
        assert_eq!(rows(&a), ["0:10=100 CLA 00", "1:11=200 ADD 00"]);
        assert_eq!(
            diagnostics(&a),
            [
                "2 [0,2] D cell 10 is on line 1 too",
                "3 [0,2] D cell 11 is on line 2 too"
            ]
        );
    }

    #[test]
    fn labels_resolve_or_are_flagged() {
        let a = assemble("10 CLA nowhere\n11 JMP loop+1\n12 loop HRS 00\n13 JMP loop+\n14 CLA x-1\n");
        assert_eq!(rows(&a), ["1:11=813 JMP loop+1", "2:12=900 loop HRS 00"]);
        assert_eq!(
            diagnostics(&a),
            [
                "0 [7,14] unknown label nowhere",
                "3 [7,12] unknown label loop+",
                "4 [7,10] unknown label x-1"
            ]
        );
        let a = assemble("10 a CLA 00\n11 a ADD 00\n12 a: SUB 00\n13 b: DATA a\n");
        assert_eq!(rows(&a), ["0:10=100 a CLA 00", "3:13=10 b DATA a"]);
        assert_eq!(
            diagnostics(&a),
            ["1 [3,4] label a is defined twice", "2 [3,5] label a is defined twice"]
        );
    }

    #[test]
    fn data_and_signs() {
        let a = assemble("10 x DAT 5\n11 DATA -999\n12 DATA 1000\n13 DATA\n14 y: DATA +7\n15 DAT -0\n");
        assert_eq!(
            rows(&a),
            [
                "0:10=5 x DATA 5",
                "1:11=-999 DATA -999",
                "4:14=7 y DATA +7",
                "5:15=0 DATA -0"
            ]
        );
        assert_eq!(
            diagnostics(&a),
            ["2 [8,12] DATA out of -999..999", "3 [3,7] DATA with no value"]
        );
        let a = assemble("10 +120 CLA 20\n11 -120 CLA 20\n12 -000\n13 +5\n14 -05\n15 +0005\n20 DATA -5\n");
        assert_eq!(
            rows(&a),
            ["0:10=120 CLA 20", "1:11=-120 CLA 20", "2:12=0", "6:20=-5 DATA -5"]
        );
        assert_eq!(
            diagnostics(&a),
            [
                "1 [3,7] W contents \u{2212}120, but CLA 20 is +120: loading the contents",
                "3 [3,5] the contents is three digits (+005)",
                "4 [3,6] the contents is three digits (\u{2212}005)",
                "5 [3,8] the contents is three digits (+005)"
            ]
        );
        assert_eq!(a.diagnostics[0].want, Some(120));
    }

    #[test]
    fn contents_out_of_range_operands_and_addresses_are_flagged() {
        let a = assemble("10 1234\n11 12\n12 0120 CLA 20\n13 99999999999999999999999\n14 5 ehst rbrt21 OUT ko\n");
        assert!(a.rows.is_empty() && a.start.is_none());
        assert_eq!(
            diagnostics(&a),
            [
                "0 [3,7] contents out of -999..999",
                "1 [3,5] the contents is three digits (+012)",
                "2 [3,7] the contents is three digits (+120)",
                "3 [3,26] contents out of -999..999",
                "4 [3,4] the contents is three digits (+005)"
            ]
        );
        let a = assemble("10 CLA 100\n11 CLA -1\n12 CLA 99\n13 CLA x\n50 x DATA 0\n14 JMP far\n150 far DATA 0\n");
        assert_eq!(rows(&a), ["2:12=199 CLA 99", "3:13=150 CLA x", "4:50=0 x DATA 0"]);
        assert_eq!(
            diagnostics(&a),
            [
                "0 [7,10] operand 100 is out of 00-99",
                "1 [7,9] operand -1 is out of 00-99",
                "5 [7,10] operand far is out of 00-99",
                "6 [0,3] the address is not a cell 00-99"
            ]
        );
    }

    #[test]
    fn cell_00_is_wired_to_001() {
        let a = assemble("00 001\n");
        assert_eq!(rows(&a), ["0:0=1"]);
        let a = assemble("00 002\n01 CLA 01\n00 001\n");
        assert_eq!(
            diagnostics(&a),
            [
                "0 [3,6] cell 00 is wired to 001: not loaded",
                "2 [0,2] D cell 00 is on line 1 too"
            ]
        );
        assert_eq!(
            diagnostics(&assemble("00 CLA 01\n")),
            ["0 [0,2] cell 00 is wired to 001: not loaded"]
        );
    }

    #[test]
    fn contents_that_disagree_with_the_opcode_load_with_a_warning() {
        let a = assemble("10 199 CLA 20\n11 820 JMP x\n20 x DATA 0\n12 007 DAT 7\n13 -07 DATA -7\n");
        assert_eq!(
            rows(&a),
            ["0:10=199 CLA 20", "1:11=820 JMP x", "2:20=0 x DATA 0", "3:12=7 DATA 7"]
        );
        assert_eq!(
            diagnostics(&a),
            [
                "0 [3,6] W contents +199, but CLA 20 is +120: loading the contents",
                "4 [3,6] the contents is three digits (\u{2212}007)"
            ]
        );
        assert_eq!(a.diagnostics[0].want, Some(120));
    }

    #[test]
    fn what_follows_the_contents_is_a_comment_unless_it_is_an_instruction() {
        let a = assemble(
            "10 120 CLA 20 extra words here\n11 CLA \"abc\"\n12 120 CLA \"abc\"\n13 STO loop: x\n14 lab OUT\n\
             15 120 lab OUT\n16 cla 20\n",
        );
        assert_eq!(
            rows(&a),
            [
                "0:10=120 CLA 20 ;extra words here",
                "2:12=120 ;CLA \"abc\"",
                "4:14=500 lab OUT 00"
            ]
        );
        assert_eq!(
            diagnostics(&a),
            [
                "1 [7,12] unknown label \"abc\"",
                "3 [7,12] unknown label loop:",
                "5 [7,10] label lab is defined twice"
            ]
        );
    }

    #[test]
    fn a_line_naming_an_opcode_that_is_not_a_cell_is_flagged() {
        let a = assemble(
            "Program No. 4\n14  N(start)  N = number of pebbles\n10 foo bar CLA 7\n7 x DATA\n 3  CLA \"abc\" prose\n\
             ADDRESS CONTENTS COMMENTS\n12 HRS",
        );
        assert_eq!(rows(&a), ["6:12=900 HRS 00"]);
        assert_eq!(
            diagnostics(&a),
            [
                "2 [0,16] cell 10: not a line of a listing (address, contents, label, opcode, operand, comment): not \
                 loaded",
                "3 [4,8] DATA with no value",
                "4 [8,13] unknown label \"abc\""
            ]
        );
    }

    #[test]
    fn non_ascii_text_is_kept_and_spans_count_utf16_for_a_page() {
        let a = assemble(
            "10 120 CLA 20 \u{201c}twenty\u{201d} \u{2013} done\n11 \u{a0}OUT 20 \u{2018}x\u{2019}\n20 DATA 7 caf\u{e9}\n\
             \u{3000} 21 DATA 8\n22\u{a0}DATA\u{a0}9\n",
        );
        assert_eq!(
            rows(&a),
            [
                "0:10=120 CLA 20 ;\u{201c}twenty\u{201d} \u{2013} done",
                "1:11=520 OUT 20 ;\u{2018}x\u{2019}",
                "2:20=7 DATA 7 ;caf\u{e9}",
                "3:21=8 DATA 8",
                "4:22=9 DATA 9"
            ]
        );
        // U+0085 isn't whitespace to a web page; U+FEFF is
        assert_eq!(tokenize("10\u{85}CLA 20"), Line::Prose(0));
        assert!(matches!(tokenize("\u{feff}10 CLA 20"), Line::Cell(_)));
        let line = "\u{a0}10 \u{1f600} CLA 7";
        let a = assemble(line);
        assert_eq!(a.diagnostics[0].span, 2..line.len());
        assert_eq!((utf16(line, 2), utf16(line, line.len())), (1, 12));
    }

    #[test]
    fn the_columns_of_a_line() {
        assert_eq!(tokenize("  \t"), Line::Blank);
        assert_eq!(tokenize("  ; 10 CLA 20"), Line::Comment(2));
        assert_eq!(tokenize(" # x"), Line::Comment(1));
        assert_eq!(tokenize("Program No. 1"), Line::Prose(0));
        assert_eq!(tokenize("10;CLA"), Line::Prose(0));
        assert_eq!(
            tokenize("10 120 loop: CLA x+1 again"),
            Line::Cell(Cell {
                address: 0..2,
                contents: Some(3..6),
                label: Some(7..12),
                opcode: Some(13..16),
                operand: Some(17..20),
                comment: Some(21),
            })
        );
        // with no contents, the word after an opcode is its operand, whatever it is
        assert_eq!(
            tokenize("10 CLA \"abc\""),
            Line::Cell(Cell {
                address: 0..2,
                opcode: Some(3..6),
                operand: Some(7..12),
                ..Cell::default()
            })
        );
    }

    #[test]
    fn where_a_listing_starts() {
        let start = |t: &str| assemble(t).start;
        assert_eq!(start("Start at 12.\n10 CLA 00\n12 HRS 00\n"), Some(12));
        // a comment isn't a line that says so
        assert_eq!(start("; Start at 12.\n10 CLA 00\n12 HRS 00\n"), Some(10));
        assert_eq!(start("START cell 7,\n10 CLA 00\n"), Some(7));
        assert_eq!(start("| Start | at | 44 |\n44 HRS 00\n"), Some(44));
        // only the first "at" or "cell" counts
        assert_eq!(start("Start the program at cell 11,\n10 CLA 00\n11 HRS 00\n"), Some(10));
        assert_eq!(
            start("Start at 150.\nstart at 12\n10 CLA 00\n11 start HRS 00\n"),
            Some(12)
        );
        assert_eq!(start("10 CLA 00\n11 start HRS 00\n"), Some(11));
        assert_eq!(start("10 DATA 5\n11 004\n12 OUT 10\n"), Some(12));
        assert_eq!(start("10 DATA 5\n11 DATA 6\n"), Some(10));
        assert_eq!(start("; nothing\n\nProgram\n"), None);
    }

    #[test]
    fn lines_split_at_line_breaks_and_trailing_blank_lines_go() {
        assert_eq!(
            lines("10 CLA 20\r\n11 HRS 00\r\n\r\n  \r\n"),
            ["10 CLA 20", "11 HRS 00"]
        );
        assert_eq!(lines("10 CLA 20\r11 HRS 00\r"), ["10 CLA 20\r11 HRS 00\r"]);
        assert_eq!(lines(""), [""]);
        assert_eq!(lines("  \n\t\n"), ["  "]);
        assert_eq!(assemble("").lines, 1);
    }
}
