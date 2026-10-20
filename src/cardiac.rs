//! The CARDIAC itself: 100 cells of three digits and a sign, an accumulator of four, and a card reader.
//!
//! Cell 00 is wired to 001, which is `INP 01`: on a cleared machine started at 00 it reads the first card into
//! cell 01 and runs on from there, so a deck that begins with the two-card loader (002, 800) boots itself. A program
//! is started that way, on cleared memory, with its tape and then its input cards in the reader. What it punches
//! (OUT) goes to a `Punch` card by card, as it is punched: collected in order, or printed at once. Past the cards
//! in the reader, a run can take more from a source of cards (`run_reading`), each asked for only when an INP
//! runs, so a source can wait for it as an operator would: a person typing, or a machine upstream still running.

/// Where the cards OUT punches go, one at a time as the program punches them: a `Vec` collects them; a host can
/// print each as it comes, or pass it on to another machine.
pub trait Punch {
    fn punch(&mut self, card: i32);
}

impl Punch for Vec<i32> {
    fn punch(&mut self, card: i32) {
        self.push(card)
    }
}

/// Why a run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Halt,
    OutOfCards,
    Limit,
}

/// How a traced run ended (`run_traced`): what a host needs to say why it stopped, such as where a panic was.
#[derive(Debug, Clone)]
pub struct Ended {
    pub stop: Stop,
    pub steps: u64,
    /// the cells of the last three instructions run, the last (the HRS, when it halted) last: so the one that led
    /// into a routine of two instructions, by a branch or by running on into it
    pub trail: [usize; 3],
    /// the phase that was running: 1, or the number of the last one loaded
    pub phase: usize,
    pub memory: [i32; 100],
}

/// The ten instructions, in the order of their first digit.
#[derive(Clone, Copy)]
enum Op {
    Inp,
    Cla,
    Add,
    Tac,
    Sft,
    Out,
    Sto,
    Sub,
    Jmp,
    Hrs,
}

/// A match, not a lookup table: `decode` runs on every write to a cell, and a table there makes the whole emulator
/// about a third slower.
#[inline(always)]
fn op(d: i32) -> Op {
    match d {
        0 => Op::Inp,
        1 => Op::Cla,
        2 => Op::Add,
        3 => Op::Tac,
        4 => Op::Sft,
        5 => Op::Out,
        6 => Op::Sto,
        7 => Op::Sub,
        8 => Op::Jmp,
        _ => Op::Hrs,
    }
}

/// The accumulator holds four digits and a sign; anything beyond is lost off the top.
#[inline(always)]
fn wrap4(v: i32) -> i32 {
    let x = v.abs() % 10000;
    if v < 0 {
        -x
    } else {
        x
    }
}

/// A cell holds three digits and a sign: STO keeps the low three.
#[inline(always)]
fn wrap3(v: i32) -> i32 {
    let x = v.abs() % 1000;
    if v < 0 {
        -x
    } else {
        x
    }
}

const POW10: [i32; 10] = [
    1, 10, 100, 1000, 10000, 100000, 1000000, 10000000, 100000000, 1000000000,
];

/// Run a cleared CARDIAC on a deck made of `parts` (read one after another), punching into `out`.
/// Returns why it stopped and how many instructions it executed.
///
/// For speed each cell's instruction is kept decoded beside its value (op, address), and re-decoded whenever
/// the cell is written, which is exactly when self-modifying code changes it.
pub fn run(parts: &[&[i32]], out: &mut impl Punch, limit: u64) -> (Stop, u64) {
    let ended = run_feeding::<false>(parts, &mut std::iter::empty(), out, limit, |_| None);
    (ended.stop, ended.steps)
}

/// `run`, for a program in phases whose input is one stream: `next(k)` gives phase k + 2's tape (k = 0, 1, ...)
/// when the program ends a phase, so it is read next, ahead of whatever input is still in the reader. A phase ends
/// with JMP 00 anywhere but cell 02 (where it is the loader's own jump); the reader is then loaded with the next
/// phase's tape.
pub fn run_phases<'a>(
    first: &'a [i32],
    input: &'a [i32],
    later: &'a [Vec<i32>],
    out: &mut impl Punch,
    limit: u64,
) -> (Stop, u64) {
    let ended = run_feeding::<false>(&[first, input], &mut std::iter::empty(), out, limit, |k| {
        later.get(k).map(|t| t.as_slice())
    });
    (ended.stop, ended.steps)
}

/// `run_phases`, keeping track of the last instructions run, for a host that reports where a run ended (a panic,
/// say).
pub fn run_traced<'a>(
    first: &'a [i32],
    input: &'a [i32],
    later: &'a [Vec<i32>],
    out: &mut impl Punch,
    limit: u64,
) -> Ended {
    run_reading::<true>(first, input, later, &mut std::iter::empty(), out, limit)
}

/// `run_phases` (`run_traced` with TRACE), and when the reader has read `first` and `input` to the end, each
/// further card INP reads is the next of `more`, asked for when that INP runs and never before; the reader is
/// empty when `more` ends.
pub fn run_reading<'a, const TRACE: bool>(
    first: &'a [i32],
    input: &'a [i32],
    later: &'a [Vec<i32>],
    more: &mut impl Iterator<Item = i32>,
    out: &mut impl Punch,
    limit: u64,
) -> Ended {
    run_feeding::<TRACE>(&[first, input], more, out, limit, |k| {
        later.get(k).map(|t| t.as_slice())
    })
}

/// The machine. With TRACE off, the instructions run are not kept track of, and cost nothing more. Once `parts`
/// are read, INP takes its cards from `more`.
#[inline(always)]
fn run_feeding<'a, const TRACE: bool>(
    parts: &[&'a [i32]],
    more: &mut impl Iterator<Item = i32>,
    out: &mut impl Punch,
    limit: u64,
    mut next: impl FnMut(usize) -> Option<&'a [i32]>,
) -> Ended {
    let mut m = [0i32; 100];
    let mut dec = [(Op::Inp, 0u8); 100];
    #[inline(always)]
    fn decode(w: i32) -> (Op, u8) {
        let w = w.abs();
        (op((w / 100) % 10), (w % 100) as u8)
    }
    m[0] = 1;
    dec[0] = decode(1);
    let (mut acc, mut pc, mut steps) = (0i32, 0usize, 0u64);
    let (mut part, mut card) = (0usize, 0usize);
    let mut cur: &[i32] = parts.first().copied().unwrap_or(&[]);
    // a phase's tape fed in ahead of the rest: where the reader was in the deck, and how many phases were fed
    let mut held: Option<(&[i32], usize)> = None;
    let mut fed = 0;
    let mut trail = [0usize; 3];
    let ended = |stop, steps, trail, fed, m| Ended {
        stop,
        steps,
        trail,
        phase: fed + 1,
        memory: m,
    };
    while steps < limit {
        steps += 1;
        if TRACE {
            trail = [trail[1], trail[2], pc];
        }
        let (op, a) = dec[pc];
        let a = a as usize;
        pc += 1;
        if pc == 100 {
            pc = 0;
        }
        match op {
            Op::Inp => {
                let next_card = loop {
                    if card < cur.len() {
                        card += 1;
                        break Some(cur[card - 1]);
                    }
                    if let Some((c, k)) = held.take() {
                        cur = c;
                        card = k;
                        continue;
                    }
                    if part + 1 < parts.len() {
                        part += 1;
                        cur = parts[part];
                        card = 0;
                        continue;
                    }
                    // the deck is read: the next card is asked for now, as this INP runs
                    break more.next();
                };
                let Some(c) = next_card else {
                    return ended(Stop::OutOfCards, steps, trail, fed, m);
                };
                if a != 0 {
                    m[a] = c;
                    dec[a] = decode(m[a]);
                }
            }
            Op::Cla => acc = m[a],
            Op::Add => acc = wrap4(acc + m[a]),
            Op::Tac => {
                if acc < 0 {
                    pc = a
                }
            }
            Op::Sft => {
                // shift left by the tens digit, then right by the units digit, keeping four digits
                let (l, r) = (a / 10, a % 10);
                let x = ((acc.abs() as i64 * POW10[l] as i64) % 10000) as i32 / POW10[r];
                acc = if acc < 0 { -x } else { x };
            }
            Op::Out => out.punch(m[a]),
            Op::Sto => {
                if a != 0 {
                    m[a] = wrap3(acc);
                    dec[a] = decode(m[a]);
                }
            }
            Op::Sub => acc = wrap4(acc - m[a]),
            Op::Jmp => {
                if a == 0 && pc != 3 {
                    // (the last phase fed has been read to its end: the reader is back in the deck)
                    if card >= cur.len() {
                        if let Some((c, k)) = held.take() {
                            cur = c;
                            card = k;
                        }
                    }
                    if held.is_none() {
                        if let Some(t) = next(fed) {
                            fed += 1;
                            held = Some((cur, card));
                            cur = t;
                            card = 0;
                        }
                    }
                }
                m[99] = 800 + pc as i32;
                dec[99] = decode(m[99]);
                pc = a;
            }
            Op::Hrs => return ended(Stop::Halt, steps, trail, fed, m),
        }
    }
    ended(Stop::Limit, steps, trail, fed, m)
}
