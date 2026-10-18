//! CARDIASM, Jason Whittington's assembly language for CARDIAC (github.com/jawhitti/Cardiac, cardiasm/Compiler.cs),
//! ported from C# to Rust. Labels name statements, `DEF name value` names a variable or constant, `//` starts a
//! comment. Statements are laid out from cell 03 up in source order; a DEF gets a cell only if a statement refers
//! to it, from cell 98 down in the order of first reference; a DEF's value may be another DEF's name. The output is
//! a .cardimg deck: the bootstrap (002, 800), an address and a word for each statement and each variable that got
//! a cell, then a jump to the first statement (002, 8xx).
//!
//! As in the original: an operand is a number, a DEF or a label; an instruction word must be 0..999 (so operands
//! are 0..99 and never negative); a DEF has one value and may be defined once. Where this port differs it says so
//! below.
use std::collections::HashMap;

/// An assembled program.
#[derive(Debug, Clone)]
pub struct Image {
    /// The deck, card by card, as the original writes a .cardimg (one value per line).
    pub deck: Vec<i32>,
    /// Every cell the program uses: address, word, and what it is (a label, a DEF or "").
    pub cells: Vec<(i32, i32, String)>,
    /// Each name's address (labels and DEFs that got a cell).
    pub symbols: HashMap<String, i32>,
    /// The first statement's address, where the deck's last card jumps.
    pub entry: i32,
}

#[derive(Debug, Clone)]
struct Statement { line: usize, op: i32, operand: String, label: Option<String> }

#[derive(Debug, Clone)]
struct Def { name: String, values: Vec<String>, address: Option<i32> }

const MIN_STATEMENT: i32 = 3;
const MAX_VAR: i32 = 98;

fn opcode(s: &str) -> Option<i32> {
    ["INP", "CLA", "ADD", "TAC", "SFT", "OUT", "STO", "SUB", "JMP", "HRS"].iter().position(|&o| o == s).map(|i| i as i32)
}

fn ident(s: &str) -> bool { !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') }
fn number(s: &str) -> Option<i32> {
    let d = s.strip_prefix('-').unwrap_or(s);
    if !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()) { s.parse().ok() } else { None }
}

/// Assemble CARDIASM source.
pub fn compile(src: &str) -> Result<Image, String> {
    let src = src.trim_start_matches('\u{feff}');
    let mut statements: Vec<Statement> = vec![];
    let mut defs: Vec<Def> = vec![];
    let mut pending: Option<String> = None;      // a label waiting for its statement
    for (n, raw) in src.lines().enumerate() {
        let line = n + 1;
        let text = raw.split("//").next().unwrap();
        let mut words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() { continue; }
        // a label, alone or before a statement ("loop:" or "loop: CLA n"; "loop :" too)
        if let Some(l) = words[0].strip_suffix(':') {
            if !ident(l) { return Err(format!("line {line}: bad label '{l}'")); }
            if pending.is_some() { return Err(format!("line {line}: two labels for one statement")); }
            pending = Some(l.to_string()); words.remove(0);
        } else if words.len() >= 2 && words[1] == ":" {
            pending = Some(words[0].to_string()); words.drain(..2);
        }
        if words.is_empty() { continue; }
        if words[0] == "DEF" {
            // (the original's grammar allows a label before DEF; it names nothing, so it is refused here)
            if pending.take().is_some() { return Err(format!("line {line}: a label before DEF")); }
            if words.len() != 3 { return Err(format!("line {line}: DEF takes a name and one value")); }
            let name = words[1];
            if !ident(name) { return Err(format!("line {line}: bad name '{name}'")); }
            if defs.iter().any(|d| d.name == name) { return Err(format!("Variable '{name}' already defined")); }
            defs.push(Def { name: name.into(), values: words[2..].iter().map(|s| s.to_string()).collect(), address: None });
            continue;
        }
        let op = opcode(words[0]).ok_or(format!("line {line}: unknown instruction '{}'", words[0]))?;
        if words.len() != 2 { return Err(format!("line {line}: {} takes one operand", words[0])); }
        statements.push(Statement { line, op, operand: words[1].into(), label: pending.take() });
    }
    if pending.is_some() { return Err("a label at the end of the program, with no statement".into()); }
    if statements.is_empty() { return Err("no statements".into()); }

    let labels: HashMap<&str, i32> = statements.iter().enumerate().filter_map(|(i, s)| s.label.as_deref().map(|l| (l, MIN_STATEMENT + i as i32))).collect();
    let end = MIN_STATEMENT + statements.len() as i32;      // the first cell after the code
    let mut next_var = MAX_VAR;                             // cells are handed out from 98 down
    let index: HashMap<String, usize> = defs.iter().enumerate().map(|(i, d)| (d.name.clone(), i)).collect();

    // a name, as a cell address; a DEF gets its cell here, on first reference
    let mut place = |defs: &mut Vec<Def>, name: &str| -> Result<i32, String> {
        if let Some(&i) = index.get(name) {
            let d = &mut defs[i];
            if d.address.is_none() { d.address = Some(next_var); next_var -= 1; }
            Ok(d.address.unwrap())
        } else if let Some(&a) = labels.get(name) { Ok(a) }
        else { Err(format!("Unknown label '{name}'")) }
    };

    let mut words = vec![];
    for (i, s) in statements.iter().enumerate() {
        let operand = match number(&s.operand) { Some(v) => v, None => place(&mut defs, &s.operand)? };
        let w = s.op * 100 + operand;
        if !(0..1000).contains(&w) || !(0..100).contains(&operand) {
            return Err(format!("line {}: '{}' is not a valid instruction", s.line, w));
        }
        words.push((MIN_STATEMENT + i as i32, w));
    }
    // the original checks room as it goes (98 cells less statements less every DEF, referenced or not); this
    // checks what matters: that the code and the cells handed out do not overlap
    if next_var < end - 1 {
        return Err(format!("Out of memory! ({} statements and {} variable cells)", statements.len(), MAX_VAR - next_var));
    }

    // DEF values: a number, or another DEF (followed to its number)
    fn resolve(defs: &[Def], index: &HashMap<String, usize>, v: &str, depth: usize) -> Result<i32, String> {
        if depth > 100 { return Err(format!("Unable to resolve identifier '{v}' (it refers to itself)")); }
        if let Some(n) = number(v) { return Ok(n); }
        match index.get(v) {
            Some(&i) => resolve(defs, index, &defs[i].values[0], depth + 1),
            None => Err(format!("Unable to resolve identifier '{v}'")),
        }
    }
    let mut cells: Vec<(i32, i32, String)> = words.iter().zip(&statements).map(|(&(a, w), s)| (a, w, s.label.clone().unwrap_or_default())).collect();
    for d in &defs {
        if let Some(a) = d.address {
            let w = resolve(&defs, &index, &d.values[0], 0)?;
            if !(-999..=999).contains(&w) { return Err(format!("DEF {}: {} does not fit in a cell", d.name, w)); }
            cells.push((a, w, d.name.clone()));
        }
    }
    let entry = MIN_STATEMENT;
    let mut deck = vec![2, 800];
    for &(a, w, _) in &cells { deck.push(a); deck.push(w); }
    deck.push(2); deck.push(800 + entry);
    let mut symbols: HashMap<String, i32> = labels.iter().map(|(k, &v)| (k.to_string(), v)).collect();
    for d in &defs { if let Some(a) = d.address { symbols.insert(d.name.clone(), a); } }
    Ok(Image { deck, cells, symbols, entry })
}

/// The deck as the original writes a .cardimg: one card a line, three digits, a space before a positive one.
pub fn cardimg(deck: &[i32]) -> String {
    deck.iter().map(|&v| if v >= 0 { format!(" {v:03}\n") } else { format!("-{:03}\n", -v) }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(src: &str, input: &[i32]) -> Vec<i32> {
        let img = compile(src).unwrap();
        let mut out = vec![];
        crate::cardiac::run(&[&img.deck, input], &mut out, 1_000_000);
        out
    }
    #[test]
    fn layout_as_the_original() {
        // statements from 03 up; DEFs from 98 down in order of first reference; unreferenced DEFs get no cell
        let img = compile("DEF a 1\nDEF unused 5\nDEF b a\nCLA b\nADD a\nSTO b\nOUT b\nHRS 0\n").unwrap();
        // (the variables follow in DEF order, as the original walks its dictionary: a, at 97, then b, at 98)
        assert_eq!(img.deck, vec![2, 800, 3, 198, 4, 297, 5, 698, 6, 598, 7, 900, 97, 1, 98, 1, 2, 803]);
        assert_eq!(cardimg(&img.deck[..3]), " 002\n 800\n 003\n");
    }
    #[test]
    fn subroutine_through_cell_99() {
        let src = "DEF one 1\nDEF two 2\nDEF t 0\nCLA one\nJMP f\nCLA two\nJMP f\nHRS 0\nf:\n STO t\n CLA 99\n STO f_exit\n OUT t\nf_exit: JMP 00\n";
        assert_eq!(run(src, &[]), vec![1, 2]);
    }
    #[test]
    fn errors() {
        assert!(compile("JMP nowhere\n").unwrap_err().contains("Unknown label"));
        assert!(compile("DEF a 1\nDEF a 2\nHRS 0\n").unwrap_err().contains("already defined"));
        assert!(compile("CLA -1\n").is_err());
        assert!(compile("DEF t 1 2\nHRS 0\n").is_err());
        assert!(compile("DEF a 1\nCLA a+1\nHRS 0\n").is_err());
    }
}
