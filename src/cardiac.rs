//! The CARDIAC itself: 100 cells of three digits and a sign, an accumulator of four, and a card reader.
//!
//! Cell 00 is wired to 001, which is `INP 01`: on a cleared machine started at 00 it reads the first card into
//! cell 01 and runs on from there, so a deck that begins with the two-card loader (002, 800) boots itself. A program
//! is started that way, on cleared memory, with its tape and then its input cards in the reader. What it punches
//! (OUT) goes to a `Console` card by card, as it is punched: collected in order, or printed at once. Past the cards
//! in the reader, a run can take more from a source of cards (`run_reading`), each asked for only when an INP
//! runs, so a source can wait for it as an operator would: a person typing, or a machine upstream still running.

/// What the machine tells its host as it runs. Above all where the cards OUT punches go, one at a time as the
/// program punches them: a `Vec` collects them; a host can print each as it comes, or pass it on to another
/// machine. A host that follows the run (a log, `cargo cardiac run -v`) is also told of each card read and each
/// phase loaded; those do nothing unless a host wants them, and then cost nothing in a run that doesn't.
pub trait Console {
    fn punch(&mut self, card: i32);

    /// `punch`, told where in the run (`punch` unless a host wants to know).
    #[inline(always)]
    fn punch_at(&mut self, card: i32, _at: At) {
        self.punch(card)
    }

    /// INP read a card: the deck's (a tape's cards are read so too, by the loader) or one from the source past it.
    #[inline(always)]
    fn read(&mut self, _card: i32, _at: At) {}

    /// The program ended a phase, and phase `phase` (2, 3, ...) has gone into the reader, ahead of the input.
    #[inline(always)]
    fn phase(&mut self, _phase: usize, _at: At) {}
}

impl Console for Vec<i32> {
    fn punch(&mut self, card: i32) {
        self.push(card)
    }
}

/// Where in a run something happened: how many instructions have run (this one among them), and the cell of the
/// instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct At {
    pub steps: u64,
    pub cell: usize,
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
    /// the accumulator when it stopped
    pub acc: i32,
}

/// The ten instructions, in the order of their first digit, and what a negative word is.
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
    /// A negative word reached as an instruction: skipped, as Brian Stuart's simulator does, which keeps the negative
    /// words free for instructions to come (#106)
    Skip,
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
/// Returns why it stopped and how many instructions it executed. HRS halts, whatever its address.
///
/// For speed each cell's instruction is kept decoded beside its value (op, address), and re-decoded whenever
/// the cell is written, which is exactly when self-modifying code changes it.
pub fn run(parts: &[&[i32]], out: &mut impl Console, limit: u64) -> (Stop, u64) {
    let ended = run_feeding::<false>(parts, &mut std::iter::empty(), out, limit, Overlay::Off, |_| None);
    (ended.stop, ended.steps)
}

/// `run`, for a program in phases in overlay mode, its phases stacked in the deck: each later tape after the cards
/// the phase before reads. At a stop for tape n (HRS n, the overlay convention) the reader holds it next, and the
/// operator starts the loader at 00; with no card left it is a halt, as is a stop with the bug on 00.
pub fn run_stacked(parts: &[&[i32]], out: &mut impl Console, limit: u64) -> (Stop, u64) {
    let ended = run_feeding::<false>(parts, &mut std::iter::empty(), out, limit, Overlay::Stacked, |_| None);
    (ended.stop, ended.steps)
}

/// How a stop with the bug on a cell other than 00 is taken: by the overlay convention, as a request for that tape,
/// or not at all (a halt, as for any program not in phases).
#[derive(Clone, Copy, PartialEq)]
enum Overlay {
    Off,
    /// the tapes are in the deck already
    Stacked,
    /// the tapes are given when asked for
    Fed,
}

/// `run`, for a program in phases whose input is one stream, run as its operator would: `next(k)` gives tape
/// k + 2 (k = 0, 1, ...). By the overlay convention a phase ends with HRS n, the machine stopped with the bug on
/// the tape to load next (on 00 it is done); the operator puts tape n in the reader, ahead of whatever input is
/// still there, and starts the loader at 00.
pub fn run_phases<'a>(
    first: &'a [i32],
    input: &'a [i32],
    later: &'a [Vec<i32>],
    out: &mut impl Console,
    limit: u64,
) -> (Stop, u64) {
    let ended = run_feeding::<false>(
        &[first, input],
        &mut std::iter::empty(),
        out,
        limit,
        Overlay::Fed,
        |k| later.get(k).map(|t| t.as_slice()),
    );
    (ended.stop, ended.steps)
}

/// `run_phases`, keeping track of the last instructions run, for a host that reports where a run ended (a panic,
/// say).
pub fn run_traced<'a>(
    first: &'a [i32],
    input: &'a [i32],
    later: &'a [Vec<i32>],
    out: &mut impl Console,
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
    out: &mut impl Console,
    limit: u64,
) -> Ended {
    run_feeding::<TRACE>(&[first, input], more, out, limit, Overlay::Fed, |k| {
        later.get(k).map(|t| t.as_slice())
    })
}

/// The machine. With TRACE off, the instructions run are not kept track of, and cost nothing more. Once `parts`
/// are read, INP takes its cards from `more`. In overlay mode, `next` gives a later phase's tape when the program
/// stops for it, unless the deck holds it already (`Overlay`).
#[inline(always)]
fn run_feeding<'a, const TRACE: bool>(
    parts: &[&'a [i32]],
    more: &mut impl Iterator<Item = i32>,
    out: &mut impl Console,
    limit: u64,
    overlay: Overlay,
    mut next: impl FnMut(usize) -> Option<&'a [i32]>,
) -> Ended {
    let mut m = [0i32; 100];
    let mut dec = [(Op::Inp, 0u8); 100];
    #[inline(always)]
    fn decode(w: i32) -> (Op, u8) {
        if w < 0 {
            return (Op::Skip, 0);
        }
        (op((w / 100) % 10), (w % 100) as u8)
    }
    m[0] = 1;
    dec[0] = decode(1);
    let (mut acc, mut pc, mut steps) = (0i32, 0usize, 0u64);
    let (mut part, mut card) = (0usize, 0usize);
    let mut cur: &[i32] = parts.first().copied().unwrap_or(&[]);
    // a phase's tape fed in ahead of the rest: where the reader was in the deck; and the phase loaded, less one
    let mut held: Option<(&[i32], usize)> = None;
    let mut fed = 0;
    let mut trail = [0usize; 3];
    let ended = |stop, steps, trail, fed, m, acc| Ended {
        stop,
        steps,
        trail,
        phase: fed + 1,
        memory: m,
        acc,
    };
    while steps < limit {
        steps += 1;
        if TRACE {
            trail = [trail[1], trail[2], pc];
        }
        let (op, a) = dec[pc];
        let a = a as usize;
        let here = pc;
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
                    return ended(Stop::OutOfCards, steps, trail, fed, m, acc);
                };
                out.read(c, At { steps, cell: here });
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
            Op::Out => out.punch_at(m[a], At { steps, cell: here }),
            Op::Sto => {
                if a != 0 {
                    m[a] = wrap3(acc);
                    dec[a] = decode(m[a]);
                }
            }
            Op::Sub => acc = wrap4(acc - m[a]),
            Op::Jmp => {
                m[99] = 800 + pc as i32;
                dec[99] = decode(m[99]);
                pc = a;
            }
            Op::Hrs => {
                // the overlay convention: a stop with the bug on n asks for tape n (the bug on 00 is done). The
                // operator puts it in the reader, ahead of whatever input is left, and starts the loader at 00
                // (the last tape fed has been read to its end by then: the reader is back in the deck)
                if card >= cur.len() {
                    if let Some((c, k)) = held.take() {
                        cur = c;
                        card = k;
                    }
                }
                // (stacked, the tape is the rest of the deck: with no card left, it is a plain halt)
                let left = card < cur.len() || parts[part + 1..].iter().any(|p| !p.is_empty());
                if a == 0 || overlay == Overlay::Off || (overlay == Overlay::Stacked && !left) {
                    return ended(Stop::Halt, steps, trail, fed, m, acc);
                }
                if overlay == Overlay::Fed {
                    let tape = if a >= 2 && held.is_none() { next(a - 2) } else { None };
                    let Some(t) = tape else {
                        return ended(Stop::Halt, steps, trail, fed, m, acc);
                    };
                    held = Some((cur, card));
                    cur = t;
                    card = 0;
                }
                fed = a - 1;
                out.phase(a, At { steps, cell: here });
                pc = 0;
            }
            Op::Skip => {}
        }
    }
    ended(Stop::Limit, steps, trail, fed, m, acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_negative_word_is_skipped() {
        // 03 CLA 07, 04 -501 (OUT 01, if run by its digits), 05 OUT 07, 06 HRS, 07 42; the loader's cards, then the
        // JMP to 03 into its cell 02 (#106)
        let deck = [2, 800, 3, 107, 4, -501, 5, 507, 6, 900, 7, 42, 2, 803];
        let mut out = vec![];
        let (stop, _) = run(&[&deck], &mut out, 1000);
        assert_eq!(stop, Stop::Halt);
        assert_eq!(out, vec![42]);
    }

    #[test]
    fn a_stacked_deck_runs_on_at_a_stop_for_a_tape() {
        // phase 1 (03 INP 10, 04 OUT 10, 05 HRS 02) and its card, 5; then tape 2 (03 INP 11, 04 OUT 11, 05 HRS 00)
        // and its card, 6: at HRS 02 the operator starts the loader at 00, and it reads tape 2 off the deck
        let one = [2, 800, 3, 10, 4, 510, 5, 902, 2, 803];
        let two = [2, 800, 3, 11, 4, 511, 5, 900, 2, 803];
        let mut out = vec![];
        let (stop, _) = run_stacked(&[&one, &[5], &two, &[6]], &mut out, 1000);
        assert_eq!((stop, out), (Stop::Halt, vec![5, 6]));
        // with no card left after the stop it is a plain halt
        let mut out = vec![];
        let (stop, _) = run_stacked(&[&one, &[5]], &mut out, 1000);
        assert_eq!((stop, out), (Stop::Halt, vec![5]));
        // and not in overlay mode, HRS 02 halts, the rest of the deck unread
        let mut out = vec![];
        let (stop, _) = run(&[&one, &[5], &two, &[6]], &mut out, 1000);
        assert_eq!((stop, out), (Stop::Halt, vec![5]));
    }
}
