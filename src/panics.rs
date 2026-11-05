//! Panic maps: where a compiled program's checks are, so whatever runs its deck can say why it panicked.
//!
//! On CARDIAC a panic is the card -999 and a halt, and that is all a machine or a deck of cards shows. A compiler
//! (rust-cardiac's `cargo cardiac build`) writes a map beside the deck, `NAME.map`, never on it, as a .pdb sits
//! beside an .exe: the deck stays CARDIAC's own. A run traced with `cardiac::run_traced` knows the last instructions
//! run; when it halts in the panic routine, the one that led there is the check that failed, and the map says what
//! it checked and in which function, as Rust reports a panic: `panicked in main: index out of bounds`.
//!
//! The file is plain text, a line each, `#` lines comments:
//!   `stop PHASE ADDRESS`                    the panic routine, in that phase
//!   `fn PHASE ADDRESS NAME`                 where a function's code starts (main's, a phase's: `main`)
//!   `check PHASE ADDRESS CALLER MESSAGE`    a branch to the panic routine, or the line that runs on into it, and
//!                                           the check it is for; CALLER is `-`, or for a runtime routine's check
//!                                           the cell that holds the return jump to the routine's caller (99, or
//!                                           the routine's exit cell), whose call it was
use crate::cardiac::{Ended, Stop};

/// How a violated unsafe precondition's check message starts, as Rust words it.
pub const VIOLATED: &str = "unsafe precondition(s) violated: ";

/// A check: a branch to the panic routine at `at`, in `phase`, taken when `message` is so.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub phase: usize,
    pub at: i32,
    /// for a runtime routine's check: the cell holding the return jump to its caller
    pub caller: Option<i32>,
    pub message: String,
}

/// A compiled program's panic map.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map {
    /// the panic routine's cell in each phase that has it
    pub stops: Vec<(usize, i32)>,
    /// where each function's code starts: (phase, cell, name)
    pub functions: Vec<(usize, i32, String)>,
    pub checks: Vec<Check>,
}

/// Why a run panicked: what failed, and in which function, as Rust says it.
#[derive(Clone, Debug, PartialEq)]
pub struct Panic {
    pub function: String,
    pub message: String,
}

impl Panic {
    /// The exit code of a run that panicked so, as `cargo run`'s: 101, as for any panic; or 134, as for SIGABRT, for
    /// a violated unsafe precondition, which Rust doesn't unwind from but aborts on.
    pub fn exit_code(&self) -> i32 {
        if self.message.starts_with(VIOLATED) {
            134
        } else {
            101
        }
    }
}

impl std::fmt::Display for Panic {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "panicked in {}: {}", self.function, self.message)
    }
}

impl Map {
    /// The `.map` file: `title` as a comment, then a line for each stop, function and check.
    pub fn text(&self, title: &str) -> String {
        let mut text = format!(
            "# {title}\n# Where the program's checks are, for a host to say why it panicked: CARDIAC never reads it.\n"
        );
        for (phase, at) in &self.stops {
            text += &format!("stop {phase} {at}\n");
        }
        for (phase, at, function) in &self.functions {
            text += &format!("fn {phase} {at} {function}\n");
        }
        for check in &self.checks {
            let caller = check.caller.map_or("-".to_string(), |cell| cell.to_string());
            text += &format!("check {} {} {caller} {}\n", check.phase, check.at, check.message);
        }
        text
    }

    /// Read a `.map` file back.
    pub fn parse(text: &str) -> Result<Map, String> {
        let mut map = Map::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let not_a_line = || format!("not a line of a panic map: {line}");
            let mut words = line.splitn(5, ' ');
            let kind = words.next().ok_or_else(not_a_line)?;
            let phase: usize = words.next().and_then(|word| word.parse().ok()).ok_or_else(not_a_line)?;
            let at: i32 = words.next().and_then(|word| word.parse().ok()).ok_or_else(not_a_line)?;
            match kind {
                "stop" => map.stops.push((phase, at)),
                "fn" => {
                    let name: Vec<&str> = words.collect();
                    map.functions.push((phase, at, name.join(" ")));
                }
                "check" => {
                    let caller = match words.next().ok_or_else(not_a_line)? {
                        "-" => None,
                        cell => Some(cell.parse().map_err(|_| not_a_line())?),
                    };
                    let message = words.next().ok_or_else(not_a_line)?.to_string();
                    map.checks.push(Check {
                        phase,
                        at,
                        caller,
                        message,
                    });
                }
                _ => return Err(not_a_line()),
            }
        }
        Ok(map)
    }

    /// The function whose code holds cell `at` in `phase`.
    fn function_at(&self, phase: usize, at: i32) -> Option<&str> {
        self.functions
            .iter()
            .filter(|(its_phase, start, _)| *its_phase == phase && *start <= at)
            .max_by_key(|(_, start, _)| *start)
            .map(|(.., function)| function.as_str())
    }

    /// Why a traced run panicked, if it did: it halted in the panic routine, and the instruction that led there is
    /// the check that failed. A run that output -999 itself and halted elsewhere did not panic.
    pub fn panic(&self, end: &Ended) -> Option<Panic> {
        if end.stop != Stop::Halt {
            return None;
        }
        let &(_, stop) = self.stops.iter().find(|(phase, _)| *phase == end.phase)?;
        // the panic routine is OUT then HRS, the last two run: the one before them led in, by a branch or running on
        let [led_in, output, halted] = end.trail.map(|cell| cell as i32);
        if (output, halted) != (stop, stop + 1) {
            return None;
        }
        let check = self
            .checks
            .iter()
            .find(|check| check.phase == end.phase && check.at == led_in);
        // a runtime routine's check is reported where it was called from: the cell before its return jump's target
        let place = match check.and_then(|check| check.caller) {
            Some(cell) => end
                .memory
                .get(cell as usize)
                .map(|return_jump| return_jump - 800 - 1)
                .unwrap_or(led_in),
            None => led_in,
        };
        Some(Panic {
            function: self.function_at(end.phase, place).unwrap_or("?").to_string(),
            message: check.map_or("a check failed".into(), |check| check.message.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A program of one phase: main checks at 5, falls into rt_chk at 8 (its check at 9, left by cell 99), the panic
    /// routine at 12; a helper, `pick`, from 20, checks at 21.
    fn map() -> Map {
        Map {
            stops: vec![(1, 12)],
            functions: vec![
                (1, 3, "main".into()),
                (1, 8, "rt_chk".into()),
                (1, 20, "kit::pick".into()),
            ],
            checks: vec![
                Check {
                    phase: 1,
                    at: 5,
                    caller: None,
                    message: "index out of bounds".into(),
                },
                Check {
                    phase: 1,
                    at: 9,
                    caller: Some(99),
                    message: "the value doesn't fit on a card (-999 to 999)".into(),
                },
                Check {
                    phase: 1,
                    at: 21,
                    caller: None,
                    message: "attempt to add with overflow".into(),
                },
            ],
        }
    }

    fn halted(trail: [usize; 3], ret: i32) -> Ended {
        let mut memory = [0; 100];
        memory[99] = ret;
        Ended {
            stop: Stop::Halt,
            steps: 10,
            trail,
            phase: 1,
            memory,
            acc: 0,
        }
    }

    #[test]
    fn text_reads_back() {
        let map = map();
        let text = map.text("x.map");
        assert!(text.contains("\ncheck 1 9 99 the value doesn't fit on a card (-999 to 999)\n"));
        assert!(text.contains("\nfn 1 20 kit::pick\n"));
        assert_eq!(Map::parse(&text).unwrap(), map);
        assert!(Map::parse("check 1 x - oops").is_err());
    }

    #[test]
    fn a_panic_is_the_check_that_led_into_the_panic_routine() {
        let map = map();
        let in_main = map.panic(&halted([5, 12, 13], 0)).unwrap();
        assert_eq!(in_main.to_string(), "panicked in main: index out of bounds");
        assert_eq!(in_main.exit_code(), 101);
        assert_eq!(map.panic(&halted([21, 12, 13], 0)).unwrap().function, "kit::pick");
        // rt_chk's check is its caller's: cell 99 holds the jump back to just after the call, at 6 (in main) or 22
        assert_eq!(map.panic(&halted([9, 12, 13], 807)).unwrap().function, "main");
        assert_eq!(map.panic(&halted([9, 12, 13], 823)).unwrap().function, "kit::pick");
        // a branch the map doesn't know: still a panic, the function where it was
        assert_eq!(map.panic(&halted([6, 12, 13], 0)).unwrap().message, "a check failed");
    }

    #[test]
    fn a_halt_elsewhere_is_no_panic() {
        let map = map();
        // the program output -999 itself and halted at 7
        assert_eq!(map.panic(&halted([5, 6, 7], 0)), None);
        let mut out_of_cards = halted([5, 12, 13], 0);
        out_of_cards.stop = Stop::OutOfCards;
        assert_eq!(map.panic(&out_of_cards), None);
        let mut later = halted([5, 12, 13], 0);
        later.phase = 2;
        assert_eq!(map.panic(&later), None);
    }

    #[test]
    fn a_violated_precondition_aborts() {
        let p = Panic {
            function: "main".into(),
            message: format!("{VIOLATED}usize::unchecked_sub cannot overflow"),
        };
        assert_eq!(p.exit_code(), 134);
    }
}
