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
    /// Past cell 99: the bug goes round to 00. Not an instruction, and not counted as one; it is only ever at the
    /// end of the decoded cells, so the step itself never checks for the wrap.
    Wrap,
    /// A breakpoint (`Machine`), in place of its cell's decoding: it stops the run before the cell runs. Never
    /// decoded in a plain run.
    Break,
}

/// The instruction a first digit stands for (`DECODED` is built from it).
const fn op(d: i32) -> Op {
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
    // ADD and SUB take an accumulator of -9999..9999 and a cell of -999..999, so the sum is within ten thousand of
    // the range: a compare, not a division
    debug_assert!((-10998..=10998).contains(&v));
    if v > 9999 {
        v - 10000
    } else if v < -9999 {
        v + 10000
    } else {
        v
    }
}

/// A cell holds three digits and a sign: STO keeps the low three.
#[inline(always)]
fn wrap3(v: i32) -> i32 {
    // Rust's remainder takes the dividend's sign: -1234 % 1000 is -234
    v % 1000
}

/// Every word's decoding, -999..999, worked out when HEARTBURN is compiled: a store looks its word up. A negative
/// word is skipped (#106), and so is STO 00, which can't change the wired cell 00: decided here, not at every STO.
static DECODED: [(Op, u8); 1999] = {
    let mut t = [(Op::Skip, 0u8); 1999];
    let mut w = 0;
    while w <= 999 {
        if w != 600 {
            t[(w + 999) as usize] = (op((w / 100) % 10), (w % 100) as u8);
        }
        w += 1;
    }
    t
};

const POW10: [i32; 10] = [
    1, 10, 100, 1000, 10000, 100000, 1000000, 10000000, 100000000, 1000000000,
];

/// Run a cleared CARDIAC on a deck made of `parts` (read one after another), punching into `out`.
/// Returns why it stopped and how many instructions it executed. HRS halts, whatever its address.
///
/// For speed each cell's instruction is kept decoded beside its value (op, address), and re-decoded whenever
/// the cell is written, which is exactly when self-modifying code changes it.
pub fn run(parts: &[&[i32]], out: &mut impl Console, limit: u64) -> (Stop, u64) {
    let ended = run_feeding::<false>(None, parts, &mut std::iter::empty(), out, limit, Overlay::Off, |_| None);
    (ended.stop, ended.steps)
}

/// `run`, for a program in phases in overlay mode, its phases stacked in the deck: each later tape after the cards
/// the phase before reads. At a stop for tape n (HRS n, the overlay convention) the reader holds it next, and the
/// operator starts the loader at 00; with no card left it is a halt, as is a stop with the bug on 00.
pub fn run_stacked(parts: &[&[i32]], out: &mut impl Console, limit: u64) -> (Stop, u64) {
    let ended = run_feeding::<false>(
        None,
        parts,
        &mut std::iter::empty(),
        out,
        limit,
        Overlay::Stacked,
        |_| None,
    );
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

/// `run`, for a program in phases whose input is one stream, run as its operator would. `first` is tape 1 and
/// `later` tapes 2, 3, .... By the overlay convention a phase ends with HRS n, the machine stopped with the bug on
/// the tape to load next (on 00 it is done); the operator puts a fresh copy of tape n in the reader, ahead of
/// whatever is still there, and starts the loader at 00. Any tape, in any order, as often as asked: phases can
/// loop or jump about. With no tape n it is a halt.
pub fn run_phases<'a>(
    first: &'a [i32],
    input: &'a [i32],
    later: &'a [Vec<i32>],
    out: &mut impl Console,
    limit: u64,
) -> (Stop, u64) {
    let ended = run_feeding::<false>(
        None,
        &[first, input],
        &mut std::iter::empty(),
        out,
        limit,
        Overlay::Fed,
        tapes(first, later),
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
    run_feeding::<TRACE>(
        None,
        &[first, input],
        more,
        out,
        limit,
        Overlay::Fed,
        tapes(first, later),
    )
}

/// Tape n of a program in phases: `first`, then `later`.
fn tapes<'a>(first: &'a [i32], later: &'a [Vec<i32>]) -> impl FnMut(usize) -> Option<&'a [i32]> {
    move |n| match n {
        0 => None,
        1 => Some(first),
        n => later.get(n - 2).map(|t| t.as_slice()),
    }
}

/// A program written straight into memory, the manual's short cut (a listing, `listing`), run from `pc` with
/// `parts` in the reader and then the cards of `more`, as INP asks for them. `memory` is the machine's 100 cells
/// (cell 00 is wired to 001 whatever it holds). A stop with the bug on a cell other than 00 is a halt: there are no
/// tapes to feed.
pub fn run_from(
    memory: &[i32; 100],
    pc: usize,
    parts: &[&[i32]],
    more: &mut impl Iterator<Item = i32>,
    out: &mut impl Console,
    limit: u64,
) -> Ended {
    run_feeding::<false>(Some((memory, pc)), parts, more, out, limit, Overlay::Off, |_| None)
}

/// The machine. With TRACE off, the instructions run are not kept track of, and cost nothing more. It starts
/// cleared at 00, or from `from`'s memory and cell. Once `parts` are read, INP takes its cards from `more`. In
/// overlay mode, `next` gives a later phase's tape when the program stops for it, unless the deck holds it already
/// (`Overlay`).
#[inline(always)]
fn run_feeding<'a, const TRACE: bool>(
    from: Option<(&[i32; 100], usize)>,
    parts: &[&'a [i32]],
    more: &mut impl Iterator<Item = i32>,
    out: &mut impl Console,
    limit: u64,
    overlay: Overlay,
    next: impl FnMut(usize) -> Option<&'a [i32]>,
) -> Ended {
    // a card or a cell holds -999..999, and a host that gives the machine anything else is told so here, not by an
    // index out of range in the middle of a run
    assert!(parts.iter().all(|p| p.iter().all(card)), "a card holds -999..999");
    let mut s = Core::cleared();
    if let Some((mem, at)) = from {
        assert!(
            mem.iter().all(card) && at < 100,
            "a cell holds -999..999, and the bug is on 00..99"
        );
        for (a, &w) in mem.iter().enumerate() {
            s.m[a] = w;
            s.dec[a] = decode(w);
        }
        s.m[0] = 1;
        s.dec[0] = decode(1);
        s.pc = at;
    }
    let mut reader = Deck {
        parts,
        part: 0,
        cur: parts.first().copied().unwrap_or(&[]),
        card: 0,
        held: vec![],
        more,
        overlay,
        next,
    };
    let stop = match exec::<TRACE, false>(&mut s, &mut Marks::default(), &mut reader, out, limit) {
        End::Halt => Stop::Halt,
        End::OutOfCards => Stop::OutOfCards,
        _ => Stop::Limit,
    };
    Ended {
        stop,
        steps: s.steps,
        trail: s.trail,
        phase: s.fed + 1,
        memory: s.m,
        acc: s.acc,
    }
}

/// Whether a number fits on a card (or in a cell): -999..999.
fn card(c: &i32) -> bool {
    (-999..=999).contains(c)
}

#[inline(always)]
fn decode(w: i32) -> (Op, u8) {
    DECODED[(w + 999) as usize]
}

/// The machine's state: memory, each cell decoded beside it (and a 101st entry, the wrap to 00), the accumulator,
/// the bug, the instructions run, and (for a debugger) the breakpoints.
#[derive(Clone)]
struct Core {
    m: [i32; 100],
    dec: [(Op, u8); 101],
    acc: i32,
    pc: usize,
    steps: u64,
    /// the cells of the last three instructions run (TRACE)
    trail: [usize; 3],
    /// the phase loaded, less one
    fed: usize,
    breaks: [bool; 100],
    /// (DEBUG) the tape that last wrote each cell (1, 2, ...; 0 if none has), and the step it wrote it at
    owner: [u8; 100],
    wrote_at: [u64; 100],
}

/// (DEBUG, with the `marks` feature, for a page that draws the machine) The cells run, and the cells an instruction
/// used as data (its operand, or 99 for JMP), since the marks were last cleared; and the last instruction run: its
/// cell, its word, and the accumulator before it. Without the feature, as native HEARTBURN is built, it is empty and
/// costs nothing, not even in the layout of the plain machine's code.
#[derive(Clone, Default)]
struct Marks {
    #[cfg(feature = "marks")]
    m: MarksKept,
}

#[cfg(feature = "marks")]
#[derive(Clone)]
struct MarksKept {
    ran: [bool; 100],
    used: [bool; 100],
    last: (usize, i32, i32),
}

#[cfg(feature = "marks")]
impl Default for MarksKept {
    fn default() -> Self {
        MarksKept {
            ran: [false; 100],
            used: [false; 100],
            last: (0, 0, 0),
        }
    }
}

/// What `Marks::note` changed, for `Marks::undo` when the instruction turns out not to run.
#[cfg(feature = "marks")]
type Undo = (bool, usize, bool, (usize, i32, i32));
#[cfg(not(feature = "marks"))]
type Undo = ();

impl Marks {
    /// The instruction in `here`, `op` with address `a`, about to run.
    #[inline(always)]
    #[allow(unused_variables)]
    fn note(&mut self, here: usize, op: Op, a: usize, word: i32, acc: i32) -> Undo {
        #[cfg(feature = "marks")]
        {
            let k = &mut self.m;
            let data = match op {
                Op::Inp | Op::Cla | Op::Add | Op::Out | Op::Sto | Op::Sub => a,
                Op::Jmp => 99,
                _ => 0,
            };
            let undo = (k.ran[here], data, k.used[data], k.last);
            k.ran[here] = true;
            if data != 0 {
                k.used[data] = true;
            }
            k.last = (here, word, acc);
            undo
        }
    }

    /// It didn't run after all: a breakpoint, or an INP waiting for a card.
    #[inline(always)]
    #[allow(unused_variables)]
    fn undo(&mut self, here: usize, u: Undo) {
        #[cfg(feature = "marks")]
        {
            let k = &mut self.m;
            (k.ran[here], k.last) = (u.0, u.3);
            k.used[u.1] = u.2;
        }
    }
}

impl Core {
    #[inline(always)]
    fn cleared() -> Core {
        let mut dec = [(Op::Inp, 0u8); 101];
        dec[100] = (Op::Wrap, 0);
        let mut m = [0i32; 100];
        m[0] = 1;
        dec[0] = decode(1);
        Core {
            m,
            dec,
            acc: 0,
            pc: 0,
            steps: 0,
            trail: [0; 3],
            fed: 0,
            breaks: [false; 100],
            owner: [0; 100],
            wrote_at: [0; 100],
        }
    }
}

/// Where INP's cards come from, and what a stop for tape n (HRS n, n > 0) does.
trait Reader {
    fn card(&mut self) -> Option<i32>;
    /// Put tape `n` in the reader, ahead of whatever is left, for the loader to read from 00: whether there is one.
    fn tape(&mut self, n: usize) -> bool;
}

/// The reader of a plain run: `parts` one after another, then `more`; in overlay mode tape n, by `next(n)`.
struct Deck<'a, 'm, M, N> {
    parts: &'m [&'a [i32]],
    part: usize,
    cur: &'a [i32],
    card: usize,
    /// a phase's tape fed in ahead of the rest: where the reader was, before each tape still being read
    held: Vec<(&'a [i32], usize)>,
    more: &'m mut M,
    overlay: Overlay,
    next: N,
}

impl<'a, M: Iterator<Item = i32>, N: FnMut(usize) -> Option<&'a [i32]>> Reader for Deck<'a, '_, M, N> {
    #[inline(always)]
    fn card(&mut self) -> Option<i32> {
        loop {
            if self.card < self.cur.len() {
                self.card += 1;
                return Some(self.cur[self.card - 1]);
            }
            if let Some((c, k)) = self.held.pop() {
                self.cur = c;
                self.card = k;
                continue;
            }
            if self.part + 1 < self.parts.len() {
                self.part += 1;
                self.cur = self.parts[self.part];
                self.card = 0;
                continue;
            }
            // the deck is read: the next card is asked for now, as this INP runs
            return self.more.next().inspect(|c| assert!(card(c), "a card holds -999..999"));
        }
    }

    #[inline(always)]
    fn tape(&mut self, n: usize) -> bool {
        // the tapes fed and read to their end are out of the reader
        while self.card >= self.cur.len() {
            let Some((c, k)) = self.held.pop() else {
                break;
            };
            self.cur = c;
            self.card = k;
        }
        match self.overlay {
            Overlay::Off => false,
            // stacked, the tape is the rest of the deck: with no card left, it is a plain halt
            Overlay::Stacked => self.card < self.cur.len() || self.parts[self.part + 1..].iter().any(|p| !p.is_empty()),
            Overlay::Fed => {
                let Some(t) = (self.next)(n) else {
                    return false;
                };
                assert!(t.iter().all(card), "a card holds -999..999");
                self.held.push((self.cur, self.card));
                self.cur = t;
                self.card = 0;
                true
            }
        }
    }
}

/// How `exec` ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Halt,
    OutOfCards,
    Limit,
    /// (DEBUG) at a breakpoint, before the cell's instruction
    Break,
    /// (DEBUG) tape n went into the reader, the bug on 00
    Fed(usize),
}

/// The machine, run on `s` until it stops or has run `limit` instructions in all. With DEBUG off it is the plain
/// machine and costs nothing more for the debugger: no `Break` is ever decoded, and a stop is only a halt, the
/// reader running dry or the limit. With DEBUG on, a breakpoint is a `Break` in the decoded cells, put back
/// whenever its cell is written, so it costs nothing per instruction either; an INP with no card leaves the bug on
/// it, uncounted, to run again once there is one; a stop for a tape ends the run with the tape in the reader; and a
/// halt leaves the bug on HRS's address, as the cardboard machine's does.
#[inline(always)]
fn exec<const TRACE: bool, const DEBUG: bool>(
    s: &mut Core,
    marks: &mut Marks,
    r: &mut impl Reader,
    out: &mut impl Console,
    limit: u64,
) -> End {
    let (mut acc, mut pc, mut steps, mut trail) = (s.acc, s.pc, s.steps, s.trail);
    let m = &mut s.m;
    let dec = &mut s.dec;
    let breaks = &s.breaks;
    let owner = &mut s.owner;
    let wrote_at = &mut s.wrote_at;

    // a cell just written: put its breakpoint back, and note which tape wrote it, and when
    macro_rules! rebreak {
        ($a:expr) => {
            if DEBUG {
                if breaks[$a] {
                    dec[$a] = (Op::Break, 0);
                }
                owner[$a] = (s.fed + 1) as u8;
                wrote_at[$a] = steps;
            }
        };
    }
    macro_rules! end {
        ($why:expr) => {{
            s.acc = acc;
            s.pc = pc;
            s.steps = steps;
            s.trail = trail;
            return $why;
        }};
    }
    while steps < limit {
        steps += 1;
        let before = trail;
        if TRACE {
            trail = [trail[1], trail[2], pc];
        }
        let (op, a) = dec[pc];
        let a = a as usize;
        let here = pc;
        pc += 1;
        // (DEBUG) the cell run, the cell it uses as data, and the instruction; and what a stop before it puts back
        #[allow(clippy::let_unit_value)]
        let undo = if DEBUG && here < 100 {
            marks.note(here, op, a, m[here], acc)
        } else {
            Undo::default()
        };
        match op {
            Op::Inp => {
                let Some(c) = r.card() else {
                    if DEBUG {
                        // waiting for a card: the INP runs again when there is one
                        steps -= 1;
                        trail = before;
                        pc = here;
                        marks.undo(here, undo);
                    }
                    end!(End::OutOfCards)
                };
                out.read(c, At { steps, cell: here });
                if a != 0 {
                    m[a] = c;
                    dec[a] = decode(m[a]);
                    rebreak!(a);
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
                m[a] = wrap3(acc);
                dec[a] = decode(m[a]);
                rebreak!(a);
            }
            Op::Sub => acc = wrap4(acc - m[a]),
            Op::Jmp => {
                // the way back: the next cell, which after 99 is 00 (#7)
                let back = if pc == 100 { 0 } else { pc };
                m[99] = 800 + back as i32;
                dec[99] = (Op::Jmp, back as u8);
                rebreak!(99);
                pc = a;
            }
            Op::Hrs => {
                // the overlay convention: a stop with the bug on n asks for tape n (the bug on 00 is done). The
                // operator puts it in the reader, ahead of whatever input is left, and starts the loader at 00
                if a == 0 || !r.tape(a) {
                    if DEBUG {
                        pc = a;
                    }
                    end!(End::Halt)
                }
                s.fed = a - 1;
                out.phase(a, At { steps, cell: here });
                pc = 0;
                if DEBUG {
                    end!(End::Fed(a))
                }
            }
            Op::Skip => {}
            Op::Wrap => {
                // (here is 100: the step before ran cell 99 and went on)
                steps -= 1;
                trail = before;
                pc = 0;
            }
            Op::Break => {
                // (only ever decoded with DEBUG on) stopped before the cell's instruction, which hasn't run
                steps -= 1;
                trail = before;
                pc = here;
                marks.undo(here, undo);
                end!(End::Break)
            }
        }
    }
    end!(End::Limit)
}

/// Why a `Machine` stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// HRS: the bug is on its address (on 00, or with no tape for it, the program is done)
    Halt,
    /// at a breakpoint, before its cell's instruction runs
    Break,
    /// an INP with no card in the reader: the bug is on the INP, which runs once a card is put in (`push`)
    Input,
    /// HRS n: tape n is in the reader and the bug on 00, to load it
    Tape(usize),
    /// the instructions asked for have run (a step, or a pause)
    Budget,
}

/// A CARDIAC a debugger can hold between runs: run it until it stops, step it, set breakpoints, and read and
/// write its memory, accumulator and bug, and the cards in its reader.
///
/// It is the same machine as `run`, compiled separately (DEBUG): a breakpoint replaces its cell's decoding, so
/// running to one costs nothing per instruction, and the plain machine pays nothing for any of it.
#[derive(Clone)]
pub struct Machine {
    core: Core,
    marks: Marks,
    reader: Queue,
    /// stopped with the bug where it is: a run from here runs a breakpoint's instruction rather than stopping again
    paused: bool,
}

/// A `Machine`'s reader: the cards in it, and the tapes of a program in phases, a fresh copy of tape n put in
/// ahead of the rest whenever it stops for it.
#[derive(Clone, Default)]
struct Queue {
    cards: std::collections::VecDeque<i32>,
    /// tape 1, 2, ...
    tapes: Vec<Vec<i32>>,
}

impl Reader for Queue {
    fn card(&mut self) -> Option<i32> {
        self.cards.pop_front()
    }

    fn tape(&mut self, n: usize) -> bool {
        let Some(t) = n.checked_sub(1).and_then(|i| self.tapes.get(i)) else {
            return false;
        };
        for &c in t.iter().rev() {
            self.cards.push_front(c);
        }
        true
    }
}

impl Default for Machine {
    fn default() -> Self {
        Machine::new()
    }
}

impl Machine {
    /// A cleared CARDIAC, the bug on 00, nothing in the reader.
    pub fn new() -> Machine {
        Machine {
            core: Core::cleared(),
            marks: Marks::default(),
            reader: Queue::default(),
            paused: false,
        }
    }

    /// A cleared CARDIAC with tape 1 and then `input` in the reader, and each tape of a program in phases put in
    /// when it stops for it: as `run_phases` runs it.
    pub fn with_tapes(tapes: &[Vec<i32>], input: &[i32]) -> Machine {
        let mut m = Machine::new();
        if let Some(first) = tapes.first() {
            m.push(first);
        }
        m.push(input);
        m.reader.tapes = tapes.to_vec();
        m
    }

    /// Run until a stop, at most `budget` instructions, each card punched going to `out`.
    pub fn run(&mut self, budget: u64, out: &mut impl Console) -> Event {
        if budget == 0 {
            return Event::Budget;
        }
        let limit = self.core.steps.saturating_add(budget);
        if self.paused && self.core.breaks[self.core.pc] {
            // run the breakpoint's instruction, then on
            match self.step(out) {
                Event::Budget => {}
                e => return e,
            }
        }
        let end = exec::<true, true>(&mut self.core, &mut self.marks, &mut self.reader, out, limit);
        self.stopped(end)
    }

    /// Run one instruction (a breakpoint on it doesn't stop it).
    pub fn step(&mut self, out: &mut impl Console) -> Event {
        let at = self.core.pc;
        self.core.dec[at] = decode(self.core.m[at]);
        let limit = self.core.steps + 1;
        let end = exec::<true, true>(&mut self.core, &mut self.marks, &mut self.reader, out, limit);
        if self.core.breaks[at] {
            self.core.dec[at] = (Op::Break, 0);
        }
        self.stopped(end)
    }

    fn stopped(&mut self, end: End) -> Event {
        if self.core.pc == 100 {
            self.core.pc = 0;
        }
        self.paused = true;
        match end {
            End::Halt => Event::Halt,
            End::OutOfCards => Event::Input,
            End::Limit => Event::Budget,
            End::Break => Event::Break,
            End::Fed(n) => Event::Tape(n),
        }
    }

    /// The tapes of a program in phases, tape 1 first, each put in the reader when the program stops for it (tape 1
    /// is not put in the reader now: `push` it, and the input after it).
    pub fn set_tapes(&mut self, tapes: Vec<Vec<i32>>) {
        assert!(
            tapes.iter().flatten().all(|c| (-999..=999).contains(c)),
            "a card holds -999..999"
        );
        self.reader.tapes = tapes;
    }

    /// The cells of the last three instructions run, the last last.
    pub fn trail(&self) -> [usize; 3] {
        self.core.trail
    }

    /// Put cards in the reader, after those in it.
    pub fn push(&mut self, cards: &[i32]) {
        assert!(cards.iter().all(|c| (-999..=999).contains(c)), "a card holds -999..999");
        self.reader.cards.extend(cards);
    }

    /// The next card INP will read.
    pub fn next_card(&self) -> Option<i32> {
        self.reader.cards.front().copied()
    }

    /// The cards in the reader, the next first.
    pub fn cards(&self) -> impl Iterator<Item = i32> + '_ {
        self.reader.cards.iter().copied()
    }

    /// Stop before the instruction in `cell` whenever the bug reaches it.
    pub fn set_break(&mut self, cell: usize) {
        self.core.breaks[cell] = true;
        self.core.dec[cell] = (Op::Break, 0);
    }

    pub fn clear_break(&mut self, cell: usize) {
        self.core.breaks[cell] = false;
        self.core.dec[cell] = decode(self.core.m[cell]);
    }

    pub fn is_break(&self, cell: usize) -> bool {
        self.core.breaks[cell]
    }

    pub fn memory(&self) -> &[i32; 100] {
        &self.core.m
    }

    pub fn cell(&self, cell: usize) -> i32 {
        self.core.m[cell]
    }

    /// Write a cell, as STO would (cell 00 is wired to 001 and keeps it), in the name of the tape running.
    pub fn set_cell(&mut self, cell: usize, word: i32) {
        assert!((-999..=999).contains(&word), "a cell holds -999..999");
        if cell == 0 {
            return;
        }
        self.core.owner[cell] = (self.core.fed + 1) as u8;
        self.core.wrote_at[cell] = self.core.steps;
        self.core.m[cell] = word;
        self.core.dec[cell] = if self.core.breaks[cell] {
            (Op::Break, 0)
        } else {
            decode(word)
        };
    }

    pub fn acc(&self) -> i32 {
        self.core.acc
    }

    pub fn set_acc(&mut self, acc: i32) {
        assert!((-9999..=9999).contains(&acc), "the accumulator holds -9999..9999");
        self.core.acc = acc;
    }

    /// The bug: the cell of the next instruction.
    pub fn pc(&self) -> usize {
        self.core.pc
    }

    pub fn set_pc(&mut self, cell: usize) {
        assert!(cell < 100, "a cell is 00..99");
        self.core.pc = cell;
        self.paused = false;
    }

    /// Write a program straight into memory (the manual's short cut; a listing): each cell as `set_cell` does,
    /// tape 1's.
    pub fn load_cells(&mut self, cells: &[(usize, i32)]) {
        for &(a, w) in cells {
            self.set_cell(a, w);
        }
    }

    /// The tape that last wrote each cell: 1, 2, ..., or 0 for a cell nothing has written. (A cell a program
    /// writes is its tape's: the one loaded last.)
    pub fn owners(&self) -> &[u8; 100] {
        &self.core.owner
    }

    /// The step each cell was last written at (0 for none): the cells the last step wrote hold `steps()`.
    pub fn written_at(&self) -> &[u64; 100] {
        &self.core.wrote_at
    }

    /// (With the `marks` feature) The cells run since the marks were last cleared (`clear_marks`).
    #[cfg(feature = "marks")]
    pub fn ran(&self) -> &[bool; 100] {
        &self.marks.m.ran
    }

    /// (With the `marks` feature) The cells an instruction used as data since the marks were last cleared: CLA,
    /// ADD, SUB, OUT, STO and INP's operands, and cell 99 for JMP.
    #[cfg(feature = "marks")]
    pub fn used(&self) -> &[bool; 100] {
        &self.marks.m.used
    }

    #[cfg(feature = "marks")]
    pub fn clear_marks(&mut self) {
        self.marks.m.ran = [false; 100];
        self.marks.m.used = [false; 100];
    }

    /// (With the `marks` feature) The last instruction run: its cell, its word then, and the accumulator before it
    /// ran.
    #[cfg(feature = "marks")]
    pub fn last(&self) -> (usize, i32, i32) {
        self.marks.m.last
    }

    /// The instructions run so far.
    pub fn steps(&self) -> u64 {
        self.core.steps
    }

    /// The phase running: 1, or the number of the last tape fed.
    pub fn phase(&self) -> usize {
        self.core.fed + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "a card holds -999..999")]
    fn a_card_too_big_is_the_hosts_mistake_and_said() {
        run(&[&[2, 800, 10, 1234, 2, 810]], &mut vec![], 1000);
    }

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
    fn the_bug_goes_round_from_99_to_00() {
        // every JMP writes cell 99, the loader's too, so the program puts an instruction there itself: 95 CLA 90
        // (CLA 91), 96 STO 99, 97 OUT 91 (42), 98 CLA 91, 99 CLA 91, and on, past 99, to cell 00: INP 01 reads the
        // next card, 900, HRS, into 01, which runs it. Nothing is counted for the wrap
        let deck = [2, 800, 90, 191, 91, 42, 95, 190, 96, 699, 97, 591, 98, 191, 2, 895, 900];
        let mut out = vec![];
        let (stop, steps) = run(&[&deck], &mut out, 1000);
        let mut traced = vec![];
        let end = run_traced(&deck, &[], &[], &mut traced, 1000);
        assert_eq!((stop, out, traced), (Stop::Halt, vec![42], vec![42]));
        assert_eq!((end.steps, end.trail), (steps, [99, 0, 1]));
    }

    #[test]
    fn a_jmp_in_99_leaves_the_way_back_to_00() {
        // 10 CLA 20, 11 STO 99, 12 JMP 99, 13 OUT 99, 14 HRS; 20 holds JMP 13: the JMP in 99 jumps to 13 and leaves
        // 800 in 99, JMP 00, the cell after 99 (#7)
        let deck = [2, 800, 10, 120, 11, 699, 12, 899, 13, 599, 14, 900, 20, 813, 2, 810];
        let mut out = vec![];
        let (stop, _) = run(&[&deck], &mut out, 1000);
        assert_eq!((stop, out), (Stop::Halt, vec![800]));
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
