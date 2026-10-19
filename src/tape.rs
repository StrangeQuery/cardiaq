//! Tapes: a compiled program as the cards CARDIAC loads it from, and the `.tape` file they are written to.
//!
//! A tape starts with the two-card loader (002, 800), then an address card and a value card for each cell of the
//! program, then a card that jumps to its start (see `asm::tape`). The program's input comes after it.
//!
//! A program in phases (`overlay()`) has a tape for each phase. Each phase ends with JMP 00, the loader, and the
//! next phase's tape goes in the reader then, ahead of whatever input is left (`cardiac::run_phases`). So the input
//! is one stream, read in order whichever phase reads it, as it is natively.
//!
//! A resource (`load_resource("NAME")`) is a slot on the first phase's tape: cells whose value cards the loader
//! fills with the resource's values, when the tape is built or loaded (`Tape::bind`). A tape whose slots are all
//! bound is complete; one with a slot still empty doesn't run.
//!
//! The file is plain text, one card per line, top to bottom, as a signed number. Lines starting with `#` are
//! comments, except two:
//!   `# phase N`                         the cards after it are phase N's tape (N = 2, 3, ...)
//!   `# resource NAME ADDRESS LENGTH`    a resource's slot, those cells of the first phase's tape, still empty;
//!                                       `bound` after it when the cards hold the resource's values
use std::path::Path;

use crate::asm;
use crate::compile::machine::{Kind, Line};
use crate::compile::Phases;

/// A resource's slot: `len` cells from address `at`, for the resource `name`, and whether its cards hold the
/// resource's values yet.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub name: String,
    pub at: i32,
    pub len: usize,
    pub bound: bool,
}

/// A program's tape: one list of cards per phase, and the resources' slots on the first.
#[derive(Clone, Debug, PartialEq)]
pub struct Tape {
    pub phases: Vec<Vec<i32>>,
    pub slots: Vec<Slot>,
}

/// The slots a piece of assembly has: a resource's table label, then the array it labels, its cells the DAT lines
/// that follow until the next label.
fn slots(data: &[Line], labels: &asm::Program) -> Vec<Slot> {
    let mut found = vec![];
    for (i, line) in data.iter().enumerate() {
        let Kind::Table { label, resource } = &line.kind else {
            continue;
        };
        let first = data.get(i + 1).is_some_and(|l| matches!(l.kind, Kind::Dat(Some(_), _)));
        let rest = data
            .iter()
            .skip(i + 2)
            .take_while(|l| matches!(l.kind, Kind::Dat(None, _)))
            .count();
        let len = if first { 1 + rest } else { 0 };
        if let Some(&at) = labels.labels.get(label) {
            found.push(Slot {
                name: resource.clone(),
                at,
                len,
                bound: false,
            });
        }
    }
    found.sort_by_key(|t| t.at);
    found
}

/// Whether a word of a `.tape` comment is a number.
fn number(word: &str) -> bool {
    !word.is_empty() && word.bytes().all(|c| c.is_ascii_digit())
}

/// A resource's name: letters, digits and `_`.
fn is_name(word: &str) -> bool {
    !word.is_empty() && word.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

/// Values written as the input is, and as a resource's file holds them: numbers -999..999, separated by spaces or
/// new lines.
pub fn values(text: &str) -> Result<Vec<i32>, String> {
    text.split_whitespace()
        .map(|w| match w.parse::<i32>() {
            Ok(v) if (-999..=999).contains(&v) => Ok(v),
            _ => Err(format!("not a card: {w} (a card holds a number from -999 to 999)")),
        })
        .collect()
}

impl Tape {
    /// The tape of a compiled program, from what `split_phases` made of it, its slots empty. The first phase loads
    /// onto cleared memory, so its cells that hold 0 are left off (but not a slot's: those cards are filled in when
    /// it is bound); a later phase loads over the one before, so all of its cells are punched.
    pub fn of(r: &Phases) -> Result<Tape, String> {
        let mut phases = vec![];
        let mut found = vec![];
        let data = r.data_text();
        let data_end = asm::assemble(3, &data)?.end;
        for i in 0..r.code.len() {
            let c = r.code_text(i);
            let text = if r.code.len() == 1 {
                format!("{c}{data}")
            } else {
                format!("{data}\n{c}")
            };
            let p = asm::assemble(3, &text)?;
            if i == 0 {
                found = slots(&r.data, &p);
            }
            let in_slot = |a: i32| found.iter().any(|t| a >= t.at && a < t.at + t.len as i32);
            let cells: Vec<(i32, i32)> = p
                .cells
                .iter()
                .copied()
                .filter(|&(a, v)| {
                    if i == 0 {
                        v != 0 || in_slot(a)
                    } else {
                        r.code.len() == 1 || a >= data_end
                    }
                })
                .collect();
            let start = if i == 0 {
                "start".to_string()
            } else {
                format!("p{}", i + 1)
            };
            let at = *p.labels.get(&start).ok_or(format!("no label {start}"))?;
            phases.push(asm::tape(at, &cells));
        }
        Ok(Tape { phases, slots: found })
    }

    /// A tape of one phase.
    pub fn one(cards: Vec<i32>) -> Tape {
        Tape {
            phases: vec![cards],
            slots: vec![],
        }
    }

    /// The `.tape` file: `title` as a comment, the slots, then one card per line.
    pub fn text(&self, title: &str) -> String {
        let mut s = format!(
            "# {title}\n# CARDIAC cards, one per line, read from the top. The program's input goes after them.\n"
        );
        if self.phases.len() > 1 {
            s += "# A program in phases: each later phase's tape goes in the reader when the one before ends (JMP 00).\n";
        }
        for t in &self.slots {
            s += &format!(
                "# resource {} {} {}{}\n",
                t.name,
                t.at,
                t.len,
                if t.bound { " bound" } else { "" }
            );
        }
        for (i, p) in self.phases.iter().enumerate() {
            if i > 0 {
                s += &format!("# phase {}\n", i + 1);
            }
            for &c in p {
                s += &if c < 0 {
                    format!("-{:03}\n", -c)
                } else {
                    format!("{c:03}\n")
                };
            }
        }
        s
    }

    /// Read a `.tape` file.
    pub fn parse(text: &str) -> Result<Tape, String> {
        let (mut phases, mut slots) = (vec![vec![]], vec![]);
        for (n, line) in text.lines().enumerate() {
            let l = line.trim();
            if l.is_empty() {
                continue;
            }
            if let Some(c) = l.strip_prefix('#') {
                let words: Vec<&str> = c.split_whitespace().collect();
                let slot = |name: &str, at: &str, len: &str, bound| Slot {
                    name: name.to_string(),
                    at: at.parse().unwrap(),
                    len: len.parse().unwrap(),
                    bound,
                };
                match words.as_slice() {
                    ["phase", n] if number(n) => phases.push(vec![]),
                    ["resource", name, at, len] if is_name(name) && number(at) && number(len) => {
                        slots.push(slot(name, at, len, false))
                    }
                    ["resource", name, at, len, "bound"] if is_name(name) && number(at) && number(len) => {
                        slots.push(slot(name, at, len, true))
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
        Ok(Tape { phases, slots })
    }

    /// Fill the slot of resource `name` with `values`, one for each of its cells, in memory: a host binds each run's
    /// data so, and the loader binds a resource's file. A slot already bound is bound again.
    pub fn bind(&mut self, name: &str, values: &[i32]) -> Result<(), String> {
        let Some(slot) = self.slots.iter_mut().find(|t| t.name == name) else {
            let names: Vec<&str> = self.slots.iter().map(|t| t.name.as_str()).collect();
            return Err(match names.as_slice() {
                [] => format!("the tape loads no resource {name}: it has none"),
                _ => format!("the tape loads no resource {name}: it has {}", names.join(", ")),
            });
        };
        if values.len() != slot.len {
            return Err(format!(
                "resource {name}: {} values, and the program loads {}",
                values.len(),
                slot.len
            ));
        }
        if let Some(v) = values.iter().find(|v| !(-999..=999).contains(*v)) {
            return Err(format!(
                "resource {name}: not a card: {v} (a card holds a number from -999 to 999)"
            ));
        }
        // the first phase's loader cards: (address, value) pairs after the two-card loader, up to the JMP to its start
        let deck = &mut self.phases[0];
        let mut filled = 0;
        let mut i = 2;
        while i + 1 < deck.len() && (0..100).contains(&deck[i]) {
            let k = deck[i] - slot.at;
            if (0..slot.len as i32).contains(&k) {
                deck[i + 1] = values[k as usize];
                filled += 1;
            }
            i += 2;
        }
        if filled != slot.len {
            return Err(format!("resource {name}: its cells are not all on the tape"));
        }
        slot.bound = true;
        Ok(())
    }

    /// Bind the resource `name` from its file, `path`.
    pub fn bind_file(&mut self, name: &str, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("resource {name}: {}: {e}", path.display()))?;
        let v = values(&text).map_err(|e| format!("resource {name}: {}: {e}", path.display()))?;
        self.bind(name, &v).map_err(|e| format!("{e} ({})", path.display()))
    }

    /// Bind each slot still empty whose resource has a file in `dir`, a package's `resources/`; the names of those
    /// still empty after.
    pub fn bind_dir(&mut self, dir: &Path) -> Result<Vec<String>, String> {
        let empty: Vec<String> = self.slots.iter().filter(|t| !t.bound).map(|t| t.name.clone()).collect();
        let mut left = vec![];
        for name in empty {
            let path = dir.join(&name);
            if path.is_file() {
                self.bind_file(&name, &path)?;
            } else {
                left.push(name);
            }
        }
        Ok(left)
    }

    /// The first phase's tape, every slot bound, then the input.
    pub fn deck(&self, input: &[i32]) -> Result<Vec<i32>, String> {
        if let Some(t) = self.slots.iter().find(|t| !t.bound) {
            return Err(format!(
                "resource {} is not bound: its slot on the tape is empty",
                t.name
            ));
        }
        let mut deck = self.phases[0].clone();
        deck.extend_from_slice(input);
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

    /// A program that prints its two-cell resource, then each card of its input, until the reader is empty.
    const PRINTS: &str = "start: OUT t\n OUT t+1\nnext: INP c\n OUT c\n JMP next\nc: DAT 0\nt: DAT 0\n DAT 0\n";

    fn tape_of(src: &str, slots: Vec<Slot>) -> Tape {
        let p = asm::assemble(3, src).unwrap();
        Tape {
            phases: vec![asm::tape(p.label("start"), &p.cells)],
            slots,
        }
    }

    fn key(bound: bool) -> Slot {
        Slot {
            name: "key".into(),
            at: 9,
            len: 2,
            bound,
        }
    }

    #[test]
    fn text_reads_back() {
        let t = Tape {
            phases: vec![vec![2, 800, 3, -5, 803], vec![2, 800, 4, 900, 804]],
            slots: vec![
                key(false),
                Slot {
                    name: "map".into(),
                    at: 20,
                    len: 3,
                    bound: true,
                },
            ],
        };
        let text = t.text("x.tape");
        assert!(text.contains("\n-005\n") && text.contains("\n# phase 2\n") && text.contains("\n# resource key 9 2\n"));
        assert!(text.contains("\n# resource map 20 3 bound\n"));
        assert_eq!(Tape::parse(&text).unwrap(), t);
        assert!(Tape::parse("002\n1000\n").is_err());
    }

    #[test]
    fn a_resource_is_bound_by_name_and_never_taken_from_the_input() {
        let mut t = tape_of(PRINTS, vec![key(false)]);
        assert!(t.run(&[7, 8, 1, 2], 1000).is_err());
        assert!(t.bind("map", &[7, 8]).is_err());
        assert!(t.bind("key", &[7]).is_err());
        assert!(t.bind("key", &[7, 1000]).is_err());
        t.bind("key", &[7, 8]).unwrap();
        assert_eq!(t.run(&[1, 2], 1000).unwrap().0, vec![7, 8, 1, 2]);
        // bound again, for another run
        t.bind("key", &[-3, 4]).unwrap();
        assert_eq!(t.run(&[], 1000).unwrap().0, vec![-3, 4]);
        assert_eq!(tape_of(PRINTS, vec![]).run(&[1, 2], 1000).unwrap().0, vec![0, 0, 1, 2]);
    }

    #[test]
    fn a_later_phase_is_read_when_the_one_before_ends() {
        // phase 1 reads a card, prints it and ends (JMP 00); phase 2, loaded over it, reads the next card and prints it
        let one = asm::tape(3, &[(3, 10), (4, 510), (5, 800), (10, 0)]);
        let two = asm::tape(3, &[(3, 11), (4, 511), (5, 900)]);
        let t = Tape {
            phases: vec![one, two],
            slots: vec![],
        };
        assert_eq!(t.run(&[5, 6], 1000).unwrap().0, vec![5, 6]);
        assert_eq!(t.run(&[5, 6], 1000).unwrap().1, crate::cardiac::Stop::Halt);
    }

    #[test]
    fn values_are_read_as_the_input_is() {
        assert_eq!(values(" 3 -1\n4\n").unwrap(), vec![3, -1, 4]);
        assert!(values("3 x").is_err());
    }
}
