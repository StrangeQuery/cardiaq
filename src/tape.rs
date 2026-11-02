//! Tapes: a program as the cards CARDIAC loads it from, and the `.tape` file they are written to.
//!
//! A tape starts with the two-card loader (002, 800), then an address card and a value card for each cell of the
//! program, then the pair 002 8xx that starts it (see `listing::deck`). The program's input comes after it.
//!
//! A program in phases (`overlay()`) has a tape for each phase. Each phase ends with HRS n, a stop with the bug on
//! the tape to load next (the overlay convention; on 00 it is done), and a fresh copy of tape n goes in the reader
//! then, ahead of whatever is left, the loader started at 00 (`cardiac::run_phases`): any tape, in any order, as
//! often as asked. So the input is one stream, read in order whichever phase reads it, as it is natively.
//!
//! A resource (`load_resource("NAME")`) is a slot on the first phase's tape: cells whose value cards the loader
//! fills with the resource's values, when the tape is built or loaded (`Tape::bind`). A tape whose slots are all
//! bound is complete; one with a slot still empty doesn't run.
//!
//! On disk each tape is a plain deck that any CARDIAC player loads: one card per line, top to bottom, a signed
//! number, nothing else. `NAME.tape` is tape 1, `NAME.2.tape` tape 2, and so on (`Tape::save`, `Tape::load`). On
//! another player a person reads the bug when it stops, loads `NAME.n.tape` and starts at 00; HEARTBURN does it
//! for them. What only HEARTBURN needs, the resources' slots, is in a file beside it, `NAME.tape.meta`, a line a slot:
//!   `resource NAME ADDRESS LENGTH`      a resource's slot, those cells of tape 1, still empty; `bound` after it
//!                                       when the cards hold the resource's values
//!
//! The one-file form tapes had before is still read (`Tape::parse`): `#` lines are comments, `# phase N` starts
//! tape N, and `# resource ...` lines are slots.
use std::path::{Path, PathBuf};

use crate::cardiac::{Console, Ended, Stop};

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

/// Whether text is meant for a deck: its first line that isn't blank or a `#` comment is a single number, a card.
/// (A listing's lines are cells, an address and more, or prose.) Such text that doesn't read as a deck is a deck
/// with something wrong in it, to be said, not taken for something else.
pub fn is_deck_text(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .is_some_and(|l| number(l.strip_prefix(['+', '-']).unwrap_or(l)))
}

impl Tape {
    /// A tape of one phase.
    pub fn one(cards: Vec<i32>) -> Tape {
        Tape {
            phases: vec![cards],
            slots: vec![],
        }
    }

    /// A tape as a plain deck: one card per line.
    pub fn deck_text(cards: &[i32]) -> String {
        cards
            .iter()
            .map(|&c| {
                if c < 0 {
                    format!("-{:03}\n", -c)
                } else {
                    format!("{c:03}\n")
                }
            })
            .collect()
    }

    /// The slots, as `NAME.tape.meta` holds them (empty when there are none).
    pub fn meta_text(&self) -> String {
        self.slots
            .iter()
            .map(|t| {
                format!(
                    "resource {} {} {}{}\n",
                    t.name,
                    t.at,
                    t.len,
                    if t.bound { " bound" } else { "" }
                )
            })
            .collect()
    }

    /// Where tape `n` of the program whose tape 1 is `path` is kept: `NAME.tape`, `NAME.2.tape`, ....
    pub fn phase_path(path: &Path, n: usize) -> PathBuf {
        if n == 1 {
            return path.to_path_buf();
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("tape");
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("tape");
        path.with_file_name(format!("{stem}.{n}.{ext}"))
    }

    /// Where the slots of the program whose tape 1 is `path` are kept: `NAME.tape.meta`.
    pub fn meta_path(path: &Path) -> PathBuf {
        let mut p = path.as_os_str().to_owned();
        p.push(".meta");
        PathBuf::from(p)
    }

    /// Write each tape as a plain deck, tape 1 to `path` and tape n beside it (`phase_path`), and the slots, if
    /// any, to `meta_path`. The files written.
    pub fn save(&self, path: &Path) -> Result<Vec<PathBuf>, String> {
        let mut written = vec![];
        let mut write = |p: PathBuf, text: String| {
            std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
            written.push(p);
            Ok::<(), String>(())
        };
        for (i, cards) in self.phases.iter().enumerate() {
            write(Tape::phase_path(path, i + 1), Tape::deck_text(cards))?;
        }
        if !self.slots.is_empty() {
            write(Tape::meta_path(path), self.meta_text())?;
        }
        Ok(written)
    }

    /// Read the program whose tape 1 is `path`: its tapes `NAME.2.tape`, `NAME.3.tape`, ... beside it, up to the
    /// first missing, and its slots from `NAME.tape.meta`. A tape in the one-file form is read whole.
    pub fn load(path: &Path) -> Result<Tape, String> {
        let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
        let text = read(path)?;
        let mut tape = Tape::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        // the one-file form is known by its `# phase` and `# resource` lines; other `#` lines are only comments, as
        // carc writes at the top of a deck, and such a deck's tapes 2, 3, ... are beside it as any plain deck's are
        if tape.phases.len() > 1 || !tape.slots.is_empty() {
            return Ok(tape);
        }
        for n in 2.. {
            let p = Tape::phase_path(path, n);
            if !p.is_file() {
                break;
            }
            let t = Tape::parse(&read(&p)?).map_err(|e| format!("{}: {e}", p.display()))?;
            if t.phases.len() > 1 || !t.slots.is_empty() {
                return Err(format!("{}: not a plain deck", p.display()));
            }
            tape.phases.extend(t.phases);
        }
        let meta = Tape::meta_path(path);
        if meta.is_file() {
            let slots = Tape::parse(&read(&meta)?.lines().map(|l| format!("# {l}\n")).collect::<String>())
                .map_err(|e| format!("{}: {e}", meta.display()))?
                .slots;
            tape.slots = slots;
        }
        Ok(tape)
    }

    /// Read a tape: a plain deck, or the one-file form (comments, `# phase N`, `# resource ...`).
    pub fn parse(text: &str) -> Result<Tape, String> {
        let (mut phases, mut slots) = (vec![vec![]], vec![]);
        for (n, line) in text.lines().enumerate() {
            let l = line.trim();
            if l.is_empty() {
                continue;
            }
            if let Some(c) = l.strip_prefix('#') {
                let words: Vec<&str> = c.split_whitespace().collect();
                // a slot is cells of the machine: from a cell 00-99, no further than 99
                let slot = |name: &str, at: &str, len: &str, bound| match (at.parse::<i32>(), len.parse::<usize>()) {
                    (Ok(at), Ok(len)) if at < 100 && at as usize + len <= 100 => Ok(Slot {
                        name: name.to_string(),
                        at,
                        len,
                        bound,
                    }),
                    _ => Err(format!(
                        "line {}: resource {name}: its cells are not cells 00-99",
                        n + 1
                    )),
                };
                match words.as_slice() {
                    ["phase", n] if number(n) => phases.push(vec![]),
                    ["resource", name, at, len] if is_name(name) && number(at) && number(len) => {
                        slots.push(slot(name, at, len, false)?)
                    }
                    ["resource", name, at, len, "bound"] if is_name(name) && number(at) && number(len) => {
                        slots.push(slot(name, at, len, true)?)
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
        let Some(deck) = self.phases.first_mut() else {
            return Err("the tape has no cards".into());
        };
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
        let Some(first) = self.phases.first() else {
            return Err("the tape has no cards".into());
        };
        let mut deck = first.clone();
        deck.extend_from_slice(input);
        Ok(deck)
    }

    /// Run the tape on `input`, keeping track of where it ended (`cardiac::run_traced`): the cards punched, and how
    /// the run ended, for a panic map to say why it panicked (`panics::Map::panic`).
    pub fn run_traced(&self, input: &[i32], limit: u64) -> Result<(Vec<i32>, crate::cardiac::Ended), String> {
        let mut punched = vec![];
        let end = self.run_traced_into(input, &mut std::iter::empty(), limit, &mut punched)?;
        Ok((punched, end))
    }

    /// `run_traced`, each card punched going to `out` as it is punched, and once `input` is read, each further
    /// card the next of `more`, asked for when the INP that reads it runs.
    pub fn run_traced_into(
        &self,
        input: &[i32],
        more: &mut impl Iterator<Item = i32>,
        limit: u64,
        out: &mut impl Console,
    ) -> Result<Ended, String> {
        let first = self.deck(&[])?;
        Ok(crate::cardiac::run_reading::<true>(
            &first,
            input,
            &self.phases[1..],
            more,
            out,
            limit,
        ))
    }

    /// Run the tape on `input` and return the cards punched, why it stopped, and how many instructions it ran
    /// (the loader's among them).
    pub fn run(&self, input: &[i32], limit: u64) -> Result<(Vec<i32>, crate::cardiac::Stop, u64), String> {
        let mut punched = vec![];
        let (why, steps) = self.run_into(input, &mut std::iter::empty(), limit, &mut punched)?;
        Ok((punched, why, steps))
    }

    /// `run`, each card punched going to `out` as it is punched, and once `input` is read, each further card the
    /// next of `more`, asked for when the INP that reads it runs.
    pub fn run_into(
        &self,
        input: &[i32],
        more: &mut impl Iterator<Item = i32>,
        limit: u64,
        out: &mut impl Console,
    ) -> Result<(Stop, u64), String> {
        let first = self.deck(&[])?;
        let end = crate::cardiac::run_reading::<false>(&first, input, &self.phases[1..], more, out, limit);
        Ok((end.stop, end.steps))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listing::{self, Listing};

    /// A program that prints its two-cell resource (cells 09 and 10), then each card of its input, until the reader
    /// is empty.
    const PRINTS: &str = "03 509 OUT 09\n04 510 OUT 10\n05 008 next INP 08\n06 508 OUT 08\n07 805 JMP next\n\
                          08 000\n09 000\n10 000\n";

    fn tape_of(src: &str, slots: Vec<Slot>) -> Tape {
        let Listing::Memory(cells) = listing::parse(src).unwrap().listing else {
            panic!("not a memory listing")
        };
        Tape {
            phases: vec![listing::deck(3, &cells).unwrap()],
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
    fn tapes_save_as_plain_decks_and_load_back() {
        let t = Tape {
            phases: vec![vec![2, 800, 3, -5, 2, 803], vec![2, 800, 4, 900, 2, 804]],
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
        let dir = std::env::temp_dir().join(format!("heartburn-tape-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.tape");
        let written = t.save(&path).unwrap();
        assert_eq!(written, [path.clone(), dir.join("x.2.tape"), dir.join("x.tape.meta")]);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "002\n800\n003\n-005\n002\n803\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("x.tape.meta")).unwrap(),
            "resource key 9 2\nresource map 20 3 bound\n"
        );
        assert_eq!(Tape::load(&path).unwrap(), t);
        assert!(Tape::parse("002\n1000\n").is_err());
        // a slot past the machine's cells is an error, not a panic
        assert!(Tape::parse("# resource k 99999999999 2\n002\n").is_err());
        assert!(Tape::parse("# resource k 98 3\n002\n").is_err());
        assert!(Tape {
            phases: vec![],
            slots: vec![]
        }
        .run(&[], 10)
        .is_err());
    }

    #[test]
    fn the_one_file_form_still_reads() {
        let t =
            Tape::parse("# x.tape\n# resource key 9 2\n002\n800\n003\n-005\n803\n# phase 2\n002\n800\n904\n").unwrap();
        assert_eq!(t.phases, vec![vec![2, 800, 3, -5, 803], vec![2, 800, 904]]);
        assert_eq!(t.slots, vec![key(false)]);
    }

    /// HRS 1 feeds tape 1 again, and phases go round: 1 → 2 → 1 → 2, each phase reading a card and punching it
    /// plus its phase's number, until the input runs out.
    #[test]
    fn any_tape_in_any_order_as_often_as_asked() {
        // tape 1: 03 INP 10, 04 CLA 10, 05 ADD 11, 06 STO 10, 07 OUT 10, 08 HRS 02; 11 holds 100
        // tape 2: the same with 200, and HRS 01
        let one = listing::deck(
            3,
            &[(3, 10), (4, 110), (5, 211), (6, 610), (7, 510), (8, 902), (11, 100)],
        )
        .unwrap();
        let two = listing::deck(
            3,
            &[(3, 10), (4, 110), (5, 211), (6, 610), (7, 510), (8, 901), (11, 200)],
        )
        .unwrap();
        let t = Tape {
            phases: vec![one, two],
            slots: vec![],
        };
        let (out, stop, _) = t.run(&[1, 2, 3, 4, 5], 10_000).unwrap();
        assert_eq!((out, stop), (vec![101, 202, 103, 204, 105], Stop::OutOfCards));
        // and on the machine a debugger holds
        let mut m = crate::cardiac::Machine::with_tapes(&t.phases, &[1, 2, 3]);
        let mut out = vec![];
        use crate::cardiac::Event;
        let events: Vec<Event> = (0..4).map(|_| m.run(10_000, &mut out)).collect();
        assert_eq!(events, [Event::Tape(2), Event::Tape(1), Event::Tape(2), Event::Input]);
        assert_eq!(out, [101, 202, 103]);
    }

    /// Past the input in the reader, each card comes from `more` when an INP asks for it, never ahead: by then the
    /// program has punched everything before it (so it can be the answer to what it punched), and its end is the
    /// reader's.
    #[test]
    fn more_cards_are_asked_for_when_inp_runs() {
        use std::cell::RefCell;
        use std::rc::Rc;
        struct Shared(Rc<RefCell<Vec<i32>>>);
        impl Console for Shared {
            fn punch(&mut self, card: i32) {
                self.0.borrow_mut().push(card)
            }
        }
        let punched = Rc::new(RefCell::new(vec![]));
        let seen = punched.clone();
        let mut asked: Vec<Vec<i32>> = vec![];
        // each card is the last card punched, plus 10
        let mut more = std::iter::from_fn(|| {
            asked.push(seen.borrow().clone());
            (asked.len() <= 2).then(|| seen.borrow().last().unwrap() + 10)
        });
        let t = tape_of(PRINTS, vec![]);
        let (why, _) = t.run_into(&[5], &mut more, 1000, &mut Shared(punched.clone())).unwrap();
        assert_eq!(why, Stop::OutOfCards);
        assert_eq!(*punched.borrow(), vec![0, 0, 5, 15, 25]);
        assert_eq!(asked, vec![vec![0, 0, 5], vec![0, 0, 5, 15], vec![0, 0, 5, 15, 25]]);
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
        // phase 1 reads a card, prints it and stops for tape 2 (HRS 2); phase 2, loaded over it, reads the next card
        // and prints it
        let one = listing::deck(3, &[(3, 10), (4, 510), (5, 902), (10, 0)]).unwrap();
        let two = listing::deck(3, &[(3, 11), (4, 511), (5, 900)]).unwrap();
        let t = Tape {
            phases: vec![one, two],
            slots: vec![],
        };
        assert_eq!(t.run(&[5, 6], 1000).unwrap().0, vec![5, 6]);
        assert_eq!(t.run(&[5, 6], 1000).unwrap().1, crate::cardiac::Stop::Halt);
    }

    #[test]
    fn deck_text_is_told_by_its_first_card() {
        assert!(is_deck_text("# a comment\n\n002\n800\n"));
        assert!(is_deck_text("1234\n"));
        assert!(is_deck_text("-005\n"));
        assert!(!is_deck_text("10 CLA 20\n"));
        assert!(!is_deck_text("Program No. 1\n002\n"));
        assert!(!is_deck_text("# only comments\n"));
    }

    #[test]
    fn values_are_read_as_the_input_is() {
        assert_eq!(values(" 3 -1\n4\n").unwrap(), vec![3, -1, 4]);
        assert!(values("3 x").is_err());
    }
}
