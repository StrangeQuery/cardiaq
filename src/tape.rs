//! Tapes: a compiled program as the cards CARDIAC loads it from, and the `.tape` file they are written to.
//!
//! A tape starts with the two-card loader (002, 800), then an address card and a value card for each cell of the
//! program, then a card that jumps to its start (see `asm::tape`). The program's input comes after it.
//!
//! A program in phases (`overlay()`) has a tape for each phase. Each phase ends with JMP 00, the loader, and the
//! next phase's tape goes in the reader then, ahead of whatever input is left (`cardiac::run_phases`). So the input
//! is one stream, read in order whichever phase reads it, as it is natively.
//!
//! The file is plain text, one card per line, top to bottom, as a signed number. Lines starting with `#` are
//! comments, except two:
//!   `# phase N`                    the cards after it are phase N's tape (N = 2, 3, ...)
//!   `# carried ADDRESS LENGTH`     a `carried()` table: those cells of the first phase's tape are filled from
//!                                  the start of the input when the tape is run (`Tape::deck`)
use crate::asm;
use crate::compile::machine::{Kind, Line};
use crate::compile::Phases;

/// A table that comes on the tape: `len` cells from address `at`.
#[derive(Clone, Debug, PartialEq)]
pub struct Table { pub at: i32, pub len: usize }

/// A program's tape: one list of cards per phase.
#[derive(Clone, Debug, PartialEq)]
pub struct Tape { pub phases: Vec<Vec<i32>>, pub tables: Vec<Table> }

/// The tables a piece of assembly carries: a label the compiler marks `; carried`, then the array it labels, its
/// cells the DAT lines that follow until the next label.
fn tables(data: &[Line], labels: &asm::Program) -> Vec<Table> {
    let mut found = vec![];
    for (i, line) in data.iter().enumerate() {
        let Some(name) = carried_label(line) else { continue };
        let first = data.get(i + 1).is_some_and(|l| matches!(l.kind, Kind::Dat(Some(_), _)));
        let rest = data.iter().skip(i + 2).take_while(|l| matches!(l.kind, Kind::Dat(None, _))).count();
        let len = if first { 1 + rest } else { 0 };
        if let Some(&at) = labels.labels.get(name) {
            found.push(Table { at, len });
        }
    }
    found.sort_by_key(|t| t.at);
    found
}

/// The label's name, when the line is a label marked `; carried` (`main_t:   ; carried: a table on the tape`).
fn carried_label(line: &Line) -> Option<&str> {
    let Kind::Label(name) = &line.kind else { return None };
    let rest = line.comment.as_ref()?.text.strip_prefix(" carried")?;
    // the word itself, not one that starts with it
    let whole = !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_');
    whole.then_some(name.as_str())
}

/// Whether a word of a `.tape` comment is a number.
fn number(word: &str) -> bool {
    !word.is_empty() && word.bytes().all(|c| c.is_ascii_digit())
}

impl Tape {
    /// The tape of a compiled program, from what `split_phases` made of it. The first phase loads onto cleared
    /// memory, so its cells that hold 0 are left off (but not a table's: those cards are filled in at run time);
    /// a later phase loads over the one before, so all of its cells are punched.
    pub fn of(r: &Phases) -> Result<Tape, String> {
        let mut phases = vec![];
        let mut tabs = vec![];
        let data = r.data_text();
        let data_end = asm::assemble(3, &data)?.end;
        for i in 0..r.code.len() {
            let c = r.code_text(i);
            let text = if r.code.len() == 1 { format!("{c}{data}") } else { format!("{data}\n{c}") };
            let p = asm::assemble(3, &text)?;
            if i == 0 { tabs = tables(&r.data, &p); }
            let table = |a: i32| tabs.iter().any(|t| a >= t.at && a < t.at + t.len as i32);
            let cells: Vec<(i32, i32)> = p.cells.iter().copied()
                .filter(|&(a, v)| if i == 0 { v != 0 || table(a) } else { r.code.len() == 1 || a >= data_end }).collect();
            let start = if i == 0 { "start".to_string() } else { format!("p{}", i + 1) };
            let at = *p.labels.get(&start).ok_or(format!("no label {start}"))?;
            phases.push(asm::tape(at, &cells));
        }
        Ok(Tape { phases, tables: tabs })
    }

    /// A tape of one phase.
    pub fn one(cards: Vec<i32>) -> Tape { Tape { phases: vec![cards], tables: vec![] } }

    /// The `.tape` file: `title` as a comment, the tables, then one card per line.
    pub fn text(&self, title: &str) -> String {
        let mut s = format!("# {title}\n# CARDIAC cards, one per line, read from the top. The program's input goes after them.\n");
        if self.phases.len() > 1 { s += "# A program in phases: each later phase's tape goes in the reader when the one before ends (JMP 00).\n"; }
        for t in &self.tables { s += &format!("# carried {} {}\n", t.at, t.len); }
        for (i, p) in self.phases.iter().enumerate() {
            if i > 0 { s += &format!("# phase {}\n", i + 1); }
            for &c in p { s += &if c < 0 { format!("-{:03}\n", -c) } else { format!("{c:03}\n") }; }
        }
        s
    }

    /// Read a `.tape` file.
    pub fn parse(text: &str) -> Result<Tape, String> {
        let (mut phases, mut tables) = (vec![vec![]], vec![]);
        for (n, line) in text.lines().enumerate() {
            let l = line.trim();
            if l.is_empty() { continue; }
            if let Some(c) = l.strip_prefix('#') {
                let words: Vec<&str> = c.split_whitespace().collect();
                match words.as_slice() {
                    ["phase", n] if number(n) => phases.push(vec![]),
                    ["carried", at, len] if number(at) && number(len) => {
                        tables.push(Table { at: at.parse().unwrap(), len: len.parse().unwrap() });
                    }
                    // any other comment
                    _ => {}
                }
                continue;
            }
            match l.parse::<i32>() {
                Ok(v) if (-999..=999).contains(&v) => phases.last_mut().unwrap().push(v),
                _ => return Err(format!("line {}: not a card: {l}", n + 1)),
            }
        }
        Ok(Tape { phases, tables })
    }

    /// The first phase's tape with its tables filled from the start of `input`, then the rest of the input.
    pub fn deck(&self, input: &[i32]) -> Result<Vec<i32>, String> {
        let need: usize = self.tables.iter().map(|t| t.len).sum();
        if input.len() < need { return Err(format!("the tape carries {need} table cards, and the input has only {}", input.len())); }
        let mut deck = self.phases[0].clone();
        let mut next = input[..need].iter();
        // the first phase's loader cards: (address, value) pairs after the two-card loader, up to the JMP to its start
        let mut values = std::collections::HashMap::new();
        for t in &self.tables { for k in 0..t.len as i32 { values.insert(t.at + k, *next.next().unwrap()); } }
        let mut i = 2;
        while i + 1 < deck.len() && (0..100).contains(&deck[i]) {
            if let Some(v) = values.remove(&deck[i]) { deck[i + 1] = v; }
            i += 2;
        }
        if let Some(a) = values.keys().min() { return Err(format!("cell {a:02} of a carried table is not on the tape")); }
        deck.extend_from_slice(&input[need..]);
        Ok(deck)
    }

    /// Run the tape on `input` and return the cards punched, why it stopped, and how many instructions it ran
    /// (the loader's among them).
    pub fn run(&self, input: &[i32], limit: u64) -> Result<(Vec<i32>, crate::cardiac::Stop, u64), String> {
        let deck = self.deck(input)?;
        let mut punched = vec![];
        let (why, steps) = crate::cardiac::run_phases(&deck, &[], &self.phases[1..], &mut punched, limit);
        Ok((punched, why, steps))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A program that prints its two-cell table, then each card of its input, until the reader is empty.
    const PRINTS: &str = "start: OUT t\n OUT t+1\nnext: INP c\n OUT c\n JMP next\nc: DAT 0\nt: DAT 0\n DAT 0\n";

    fn tape_of(src: &str, tables: Vec<Table>) -> Tape {
        let p = asm::assemble(3, src).unwrap();
        Tape { phases: vec![asm::tape(p.label("start"), &p.cells)], tables }
    }

    #[test]
    fn text_reads_back() {
        let t = Tape { phases: vec![vec![2, 800, 3, -5, 803], vec![2, 800, 4, 900, 804]], tables: vec![Table { at: 9, len: 2 }] };
        let text = t.text("x.tape");
        assert!(text.contains("\n-005\n") && text.contains("\n# phase 2\n") && text.contains("\n# carried 9 2\n"));
        assert_eq!(Tape::parse(&text).unwrap(), t);
        assert!(Tape::parse("002\n1000\n").is_err());
    }

    #[test]
    fn tables_come_from_the_input() {
        let t = tape_of(PRINTS, vec![Table { at: 9, len: 2 }]);
        assert_eq!(t.run(&[7, 8, 1, 2], 1000).unwrap().0, vec![7, 8, 1, 2]);
        assert!(t.run(&[7], 1000).is_err());
        assert_eq!(tape_of(PRINTS, vec![]).run(&[1, 2], 1000).unwrap().0, vec![0, 0, 1, 2]);
    }

    #[test]
    fn a_later_phase_is_read_when_the_one_before_ends() {
        // phase 1 reads a card, prints it and ends (JMP 00); phase 2, loaded over it, reads the next card and prints it
        let one = asm::tape(3, &[(3, 10), (4, 510), (5, 800), (10, 0)]);
        let two = asm::tape(3, &[(3, 11), (4, 511), (5, 900)]);
        let t = Tape { phases: vec![one, two], tables: vec![] };
        assert_eq!(t.run(&[5, 6], 1000).unwrap().0, vec![5, 6]);
        assert_eq!(t.run(&[5, 6], 1000).unwrap().1, crate::cardiac::Stop::Halt);
    }
}
