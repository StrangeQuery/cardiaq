//! ekg: a debugger and a profiler for CARDIAC programs. It holds a `cardiac::Machine` and drives it an instruction
//! at a time (`step`), so it sees each instruction run and each cell written, and counts what it needs itself: the
//! machine runs exactly as it does in `burn`, which pays nothing for any of it.
//!
//! A program is a listing (assembled as carc assembles it, its labels, opcodes and comments kept) or a deck (shown
//! disassembled). Either is loaded by its deck's own loader, as burn loads it: a listing as carc's deck of it, so it
//! runs in ekg exactly as it runs in burn.
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::time::Instant;

use heartburn::assemble::{self, written, Row, OPS};
use heartburn::cardiac::{Console, Event, Machine};
use heartburn::listing::{self, Notes};
use heartburn::tape::{values, Tape};

#[path = "../cli.rs"]
mod cli;
// (its `parse`, which exits on a mistake, is burn's and carc's; ekg reads its options with `try_parse`)
#[allow(dead_code)]
#[path = "../getopt.rs"]
mod getopt;

/// Instructions a `continue` (or a profile) may run before ekg stops it as a program that may never stop.
const LIMIT: u64 = 1_000_000_000;

fn usage() -> &'static str {
    "\
Usage: ekg [OPTION]... FILE [INPUT]...
Debug a CARDIAC program, or profile it (--profile). FILE is a listing (NAME.cq, assembled as carc assembles
it) or a deck (loaded by its own loader, as burn loads it), told apart by what is in it. INPUT is the cards in the
reader when it starts (after --, so that negative cards aren't taken for options).

Debugging, commands are read from standard input, a line each (the command help lists them), after those given
with -x. What the program punches is printed as it is punched, a card a line, each starting `out:`.

Profiling, the program runs to the end and the report goes to standard output: the instructions run, the hottest
cells, each cell's runs beside the listing, the cells the program wrote (and which of those it ran too: code it
rewrites), the instructions it never ran. Its cards go to standard error. With no INPUT (and no --), its cards are
read from standard input, each when an INP asks for it, as burn reads them.

  -x, --eval-command=COMMAND  carry out COMMAND (any number of times, in order)
      --batch                 carry out the -x commands, then exit, without reading standard input
  -q, --quiet, --silent       no introduction
      --profile               profile instead of debugging
      --limit=N               stop a run after N instructions (default 1000000000)
      --start-at=NN           start a listing at cell NN (default: where carc would start it)
      --help                  display this help and exit
      --version               output version information and exit

Exit status is 0 on success, 1 when something failed (a listing with errors; with --batch, a command; in a
profile, a run stopped at the limit), and 2 for a usage error.

Report bugs to: https://github.com/StrangeQuery/cardiaq/issues
"
}

fn help() -> &'static str {
    "\
Commands (a PLACE is a cell, 00-99, a label, or label+N):
  run, r                 start the program again from the beginning, with its INPUT, and continue
  continue, c            run until a breakpoint, a watch, a halt, or an INP with no card
  step, s [N]            run one instruction (or N)
  next, n [N]            step, but run a routine a JMP calls through: on to the cell after the JMP (a JMP
                         calls when the code it jumps to reads 99, the way back, before another JMP)
  break, b PLACE         stop before the instruction in PLACE runs
  watch PLACE            stop after an instruction writes PLACE
  delete, d [PLACE]      remove the breakpoint and watch on PLACE (all of them without one)
  info break, i b        the breakpoints and watches
  info registers, i r    the PC, the accumulator, the instructions run, the cards in the reader
  print, p PLACE         a cell: its word, and the word as an instruction (also: p acc, p pc)
  x PLACE [N]            the words in N cells from PLACE (10 unless given)
  list, l [PLACE]        the listing around the PC (or PLACE), the PC marked >, breakpoints b
  set PLACE VALUE        write a cell (also: set acc VALUE, set pc PLACE)
  input CARDS...         put cards in the reader, after those in it
  help, h                this
  quit, q                leave
An empty line repeats the last step, next or continue.
"
}

fn die(why: impl std::fmt::Display) -> ! {
    getopt::die("ekg", why)
}

fn bad_usage(why: &str) -> ! {
    getopt::usage_error("ekg", why)
}

/// A program as ekg holds it: the machine as it is when the program starts, and the listing its cells came from.
struct Program {
    /// loaded, the PC on the first instruction, the input in the reader
    start: Machine,
    /// the instructions the deck's loader ran loading it (none for a listing)
    loading: u64,
    /// the cells listed, in the order of the listing (a deck's disassembled, in the order of the cells)
    rows: Vec<Row>,
    labels: Vec<(String, usize)>,
    /// the listing's row for each cell
    row_of: [Option<usize>; 100],
}

impl Program {
    /// Load FILE, a deck or a listing (told apart by what is in it, as burn does), the input after its cards.
    fn load(file: &str, start: Option<usize>, input: &[i32]) -> Program {
        let text = std::fs::read_to_string(file).unwrap_or_else(|e| die(format!("{file}: {}", getopt::os_error(&e))));
        match cli::deck_or_not(file, &text, Some(Path::new(file))) {
            Err(e) => die(e),
            Ok(Some(_)) if start.is_some() => bad_usage(&format!(
                "{file}: --start-at is for a listing: a deck starts where its loader jumps"
            )),
            Ok(Some(deck)) => Program::from_deck(file, deck, input),
            Ok(None) => Program::from_listing(file, &text, start, input),
        }
    }

    /// A listing, run as carc's deck of it: the loader takes cells 01 and 02 and its jumps write 99, as in burn
    /// (and ekg warns of a listing that puts anything there, as carc does).
    fn from_listing(file: &str, text: &str, start: Option<usize>, input: &[i32]) -> Program {
        let asm = assemble::assemble(text);
        if cli::report("ekg: ", file, text, &asm, true) {
            die(format!("{file}: not assembled"))
        }
        let Some(start) = start.or(asm.start) else {
            die(format!("{file}: no cells: nothing to run"))
        };
        if let Some(w) = cli::start_warning(start, &asm) {
            eprintln!("ekg: {file}: warning: {w}");
        }
        let m = Program::boot(file, &[assemble::deck(&asm, start)], input);
        let loading = m.steps();
        Program::new(m, loading, asm.rows)
    }

    /// A cleared machine with the tapes and then `input` in the reader, its loader run until it hands over to the
    /// program (or waits for a card: a deck that ends in the manual's BLANK card).
    fn boot(file: &str, phases: &[Vec<i32>], input: &[i32]) -> Machine {
        let mut m = Machine::with_tapes(phases, input);
        while m.pc() <= 2 && m.steps() < 10 * phases[0].len() as u64 + 10 {
            match m.step(&mut vec![]) {
                Event::Budget => {}
                Event::Input => break,
                e => die(format!("{file}: the loader stopped ({e:?}) at cell {:02}", m.pc())),
            }
        }
        m
    }

    /// A deck: loaded by its loader (`boot`), and its cells disassembled as `heartburn-tools` lists them.
    fn from_deck(file: &str, mut deck: Tape, input: &[i32]) -> Program {
        let dir = Path::new(file)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        deck.bind_dir(&dir.join("resources"))
            .unwrap_or_else(|e| die(format!("{file}: {e}")));
        let first = deck.deck(&[]).unwrap_or_else(|e| die(format!("{file}: {e}")));
        let loaded = listing::from_deck(&first).unwrap_or_else(|e| die(format!("{file}: {e}")));
        let mut phases = deck.phases.clone();
        phases[0] = first;
        let m = Program::boot(file, &phases, input);
        let text = listing::six_columns(&loaded.cells, loaded.start, &Notes::default());
        let loading = m.steps();
        Program::new(m, loading, assemble::assemble(&text).rows)
    }

    fn new(start: Machine, loading: u64, rows: Vec<Row>) -> Program {
        let mut row_of = [None; 100];
        let mut labels = vec![];
        for (i, r) in rows.iter().enumerate() {
            row_of[r.addr] = Some(i);
            if let Some(l) = &r.label {
                labels.push((l.clone(), r.addr));
            }
        }
        Program {
            start,
            loading,
            rows,
            labels,
            row_of,
        }
    }

    fn row(&self, cell: usize) -> Option<&Row> {
        self.row_of[cell].map(|i| &self.rows[i])
    }

    /// A place: a cell (one or two digits), a label, or label+N.
    fn place(&self, s: &str) -> Result<usize, String> {
        let bad = || format!("{s}: not a cell (00-99) or a label");
        if !s.is_empty() && s.len() <= 2 && s.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(s.parse().unwrap());
        }
        let (name, n) = match s.split_once('+') {
            Some((name, n)) => (name, n.parse::<usize>().map_err(|_| bad())?),
            None => (s, 0),
        };
        let at = self.labels.iter().find(|l| l.0 == name).ok_or_else(bad)?.1;
        (at + n < 100)
            .then_some(at + n)
            .ok_or_else(|| format!("{s}: past cell 99"))
    }

    /// The label on a cell, if one names it.
    fn label(&self, cell: usize) -> Option<&str> {
        self.labels.iter().find(|l| l.1 == cell).map(|l| l.0.as_str())
    }

    /// A word as an instruction: its opcode and operand, the operand named by its label when one names that cell
    /// (not SFT's shifts or HRS's stop). A negative word runs as nothing (it is skipped), so it is DATA.
    fn decode(&self, w: i32) -> (&'static str, String) {
        if w < 0 {
            return ("DATA", w.to_string());
        }
        let (op, a) = ((w / 100) as usize, (w % 100) as usize);
        let named = if matches!(op, 4 | 9) { None } else { self.label(a) };
        (OPS[op], named.map_or(format!("{a:02}"), str::to_string))
    }

    /// A cell as a line of the listing, `CELL CONTENTS LABEL OPCODE OPERAND ; COMMENT`, from what it holds now: as
    /// the listing writes it while it holds what the listing put there; decoded when the program has written it,
    /// as an instruction if the listing says it is one or `code` (it is about to run), and the listing's contents
    /// after it.
    fn line(&self, m: &Machine, cell: usize, code: bool) -> String {
        let w = m.cell(cell);
        let row = self.row(cell);
        let (op, operand) = match row {
            Some(r) if r.contents == w && r.op.is_some() && !(code && r.op == Some("DATA")) => {
                (r.op.unwrap(), r.operand.clone().unwrap_or_default())
            }
            _ => {
                let listed_code = row.is_some_and(|r| r.op.is_none_or(|o| o != "DATA"));
                if code || listed_code {
                    self.decode(w)
                } else {
                    ("DATA", w.to_string())
                }
            }
        };
        let label = row.and_then(|r| r.label.as_deref()).or(self.label(cell)).unwrap_or("");
        let mut comment = row.map_or(String::new(), |r| r.comment.clone());
        if let Some(r) = row.filter(|r| r.contents != w && op != "DATA") {
            comment = format!("{comment} (listed {})", written(r.contents)).trim().to_string();
        }
        let comment = if comment.is_empty() {
            comment
        } else {
            format!("; {comment}")
        };
        format!(
            "{cell:02}  {:>4}  {label:<8} {op:<4} {operand:<8} {comment}",
            written(w)
        )
        .trim_end()
        .to_string()
    }
}

/// The cell an instruction may write: INP's and STO's address (not 00, which is wired), and 99 for JMP, which
/// leaves the way back there.
fn target(w: i32) -> Option<usize> {
    let (op, a) = (w / 100, (w % 100) as usize);
    match op {
        _ if w < 0 => None,
        0 | 6 if a != 0 => Some(a),
        8 => Some(99),
        _ => None,
    }
}

/// The program's cards, each on a line of its own as it is punched: `out: N` among ekg's lines, or a bare number
/// on standard error in a profile.
struct Teletype {
    profile: bool,
}

impl Console for Teletype {
    fn punch(&mut self, card: i32) {
        if self.profile {
            eprintln!("{card}");
        } else {
            println!("out: {card}");
        }
    }
}

/// Why a run stopped.
enum Stop {
    /// it ran the instructions asked for
    Done,
    Break,
    /// an instruction wrote a watched cell: the cell, its word before, and the cell of the instruction
    Watch(usize, i32, usize),
    /// HRS, in this cell
    Halt(usize),
    /// an INP with no card
    Input,
    /// a routine `next` runs through ran the instructions a run may
    Limit,
}

/// Said when a program in phases stops for deck `k`.
fn fed(k: usize) {
    println!("HRS {k:02}: deck {k} goes in the reader, and the loader starts at 00");
}

/// A debugging session: the program, the machine running it, the breakpoints and watches.
struct Session<'a> {
    p: &'a Program,
    m: Machine,
    breaks: [bool; 100],
    watches: [bool; 100],
    halted: bool,
    limit: u64,
    /// the last step, next or continue, which an empty line repeats
    last: Option<String>,
    /// a command went wrong (ekg then exits 1, when it isn't interactive)
    failed: bool,
}

impl Session<'_> {
    /// Run up to `n` instructions: stop at a breakpoint (not on the first, so a continue from one goes on, unless
    /// `fresh`: `run` stops on a breakpoint at the program's start, as gdb does), after an instruction writes a
    /// watched cell, at a halt, or at an INP with no card. `over`: a JMP counts as one, the routine it calls run
    /// through (`through`).
    fn go(&mut self, n: u64, over: bool, fresh: bool) -> Stop {
        if !over && !self.watches.contains(&true) {
            return self.go_fast(n, fresh);
        }
        for i in 0..n {
            let pc = self.m.pc();
            if (i > 0 || fresh) && self.breaks[pc] {
                return Stop::Break;
            }
            let w = self.m.cell(pc);
            let stop = if over && w >= 0 && w / 100 == 8 && self.is_call((w % 100) as usize) {
                self.through(pc)
            } else {
                self.one()
            };
            if let Some(stop) = stop {
                return stop;
            }
        }
        Stop::Done
    }

    /// `go` with nothing watched and no routine to run through: the machine runs to a breakpoint itself (they are
    /// set in it, `Machine::set_break`), at no cost per instruction, rather than an instruction at a time.
    fn go_fast(&mut self, n: u64, fresh: bool) -> Stop {
        let end = self.m.steps().saturating_add(n);
        if self.breaks[self.m.pc()] {
            if fresh {
                return Stop::Break;
            }
            // on from the breakpoint it is stopped on: its instruction, then on
            if let Some(stop) = self.one() {
                return stop;
            }
            if self.breaks[self.m.pc()] {
                return Stop::Break;
            }
        }
        for c in 0..100 {
            match (self.breaks[c], self.m.is_break(c)) {
                (true, false) => self.m.set_break(c),
                (false, true) => self.m.clear_break(c),
                _ => {}
            }
        }
        loop {
            let left = end.saturating_sub(self.m.steps());
            match self.m.run(left, &mut Teletype { profile: false }) {
                Event::Budget => return Stop::Done,
                Event::Break => return Stop::Break,
                // the bug is on HRS's address: the HRS is the last instruction run
                Event::Halt => return Stop::Halt(self.m.trail()[2]),
                Event::Input => return Stop::Input,
                Event::Tape(k) => {
                    fed(k);
                    if self.m.steps() >= end {
                        return Stop::Done;
                    }
                }
            }
        }
    }

    /// Whether a JMP to `to` calls a routine, by the CARDIAC convention: the code it jumps to reads the way back the
    /// JMP leaves in 99 (`CLA 99` to keep it, or `JMP 99` to go back) before another JMP writes over it. The cells
    /// from `to` are read as they are now, in order, up to the first JMP or HRS; a TAC's branch isn't followed.
    fn is_call(&self, to: usize) -> bool {
        for c in to..100 {
            let w = self.m.cell(c);
            if w < 0 {
                continue;
            }
            match (w / 100, w % 100) {
                (1 | 2 | 5 | 7, 99) => return true,
                (8, a) => return a == 99,
                (9, _) => return false,
                _ => {}
            }
        }
        false
    }

    /// The JMP in `jmp`, and the routine it calls: on until the bug is back on the cell after the JMP, where the
    /// way back it left in 99 goes; or on the JMP again (it was a loop, not a call); or another stop.
    fn through(&mut self, jmp: usize) -> Option<Stop> {
        let back = (jmp + 1) % 100;
        if let Some(stop) = self.one() {
            return Some(stop);
        }
        for _ in 0..self.limit {
            let pc = self.m.pc();
            if pc == back || pc == jmp {
                return None;
            }
            if self.breaks[pc] {
                return Some(Stop::Break);
            }
            if let Some(stop) = self.one() {
                return Some(stop);
            }
        }
        Some(Stop::Limit)
    }

    /// Run one instruction; a stop if it wrote a watched cell, halted, or is an INP waiting for a card.
    fn one(&mut self) -> Option<Stop> {
        let pc = self.m.pc();
        let watched = target(self.m.cell(pc)).filter(|&t| self.watches[t]);
        let before = watched.map(|t| self.m.cell(t));
        let event = self.m.step(&mut Teletype { profile: false });
        if let Event::Tape(k) = event {
            fed(k);
        }
        if let (Some(t), Some(old)) = (watched, before) {
            if self.m.written_at()[t] == self.m.steps() {
                return Some(Stop::Watch(t, old, pc));
            }
        }
        match event {
            Event::Halt => Some(Stop::Halt(pc)),
            Event::Input => Some(Stop::Input),
            _ => None,
        }
    }

    /// Run, and say where it stopped: the next instruction, as the listing has it.
    fn run_for(&mut self, n: u64, stepping: bool, over: bool, fresh: bool) {
        if self.halted {
            println!("the program has halted: run starts it again");
            return;
        }
        let from = self.m.steps();
        match self.go(n, over, fresh) {
            Stop::Done if stepping => {}
            Stop::Done | Stop::Limit => println!(
                "still running after {} instructions (--limit): continue goes on",
                self.m.steps() - from
            ),
            Stop::Break => println!("breakpoint, cell {:02}", self.m.pc()),
            Stop::Watch(t, old, by) => {
                let now = self.m.cell(t);
                let change = if now == old {
                    format!("{} (unchanged)", written(now))
                } else {
                    format!("{} -> {}", written(old), written(now))
                };
                println!("watch, cell {}: {change}, written by cell {by:02}", self.name(t));
            }
            Stop::Halt(at) => {
                self.halted = true;
                println!("halted: HRS in cell {at:02}, after {} instructions", self.m.steps());
                return;
            }
            Stop::Input => {
                println!(
                    "INP in cell {:02} is waiting for a card: input CARDS, then continue",
                    self.m.pc()
                );
                return;
            }
        }
        println!("{}", self.p.line(&self.m, self.m.pc(), true));
    }

    /// A cell by number, and its label if it has one: `08 (rdi)`.
    fn name(&self, cell: usize) -> String {
        match self.p.label(cell) {
            Some(l) => format!("{cell:02} ({l})"),
            None => format!("{cell:02}"),
        }
    }

    fn restart(&mut self) {
        self.m = self.p.start.clone();
        self.halted = false;
    }

    /// The listing around a cell: the five cells listed before it and the five after (or the cells near it, when
    /// it isn't listed).
    fn list(&self, around: usize) {
        let rows = &self.p.rows;
        let at = rows.iter().position(|r| r.addr >= around).unwrap_or(rows.len());
        let mut cells: Vec<usize> = rows[at.saturating_sub(5)..(at + 5).min(rows.len())]
            .iter()
            .map(|r| r.addr)
            .collect();
        if !cells.contains(&around) {
            cells.push(around);
        }
        for c in cells {
            let pc = c == self.m.pc() && !self.halted;
            let mark = format!(
                "{}{}",
                if self.breaks[c] { 'b' } else { ' ' },
                if pc { '>' } else { ' ' }
            );
            println!("{mark} {}", self.p.line(&self.m, c, pc));
        }
    }

    /// A word, and what it is as an instruction (not for a cell the listing calls DATA).
    fn print(&self, what: &str) -> Result<(), String> {
        match what {
            "acc" => println!("acc = {:+05}", self.m.acc()),
            "pc" => println!("pc = {:02}", self.m.pc()),
            _ => {
                let c = self.p.place(what)?;
                let w = self.m.cell(c);
                let data = self.p.row_of[c].is_some_and(|r| self.p.rows[r].op == Some("DATA"));
                let as_code = if data {
                    String::new()
                } else if w < 0 {
                    "    negative: skipped if run".to_string()
                } else {
                    format!("    {} {:02}", OPS[(w / 100) as usize], w % 100)
                };
                println!("{} = {}{as_code}", self.name(c), written(w));
            }
        }
        Ok(())
    }

    /// Carry out a command line; false to quit.
    fn command(&mut self, line: &str) -> bool {
        let line = match (line.trim(), &self.last) {
            ("", Some(last)) => last.clone(),
            ("", None) => return true,
            (l, _) => l.to_string(),
        };
        let words: Vec<&str> = line.split_whitespace().collect();
        let (cmd, args) = (words[0], &words[1..]);
        if matches!(cmd, "step" | "s" | "next" | "n" | "continue" | "c") {
            self.last = Some(line.clone());
        }
        if let Err(e) = self.carry_out(cmd, args) {
            let _ = std::io::stdout().flush();
            eprintln!("{e}");
            self.failed = true;
        }
        !matches!(cmd, "quit" | "q")
    }

    fn carry_out(&mut self, cmd: &str, args: &[&str]) -> Result<(), String> {
        let one = |what: &str| match args {
            [a] => Ok(*a),
            _ => Err(format!("{cmd} {what}")),
        };
        match cmd {
            "run" | "r" => {
                self.restart();
                self.run_for(self.limit, false, false, true)
            }
            "continue" | "c" => self.run_for(self.limit, false, false, false),
            "step" | "s" | "next" | "n" => {
                let n = match args {
                    [] => 1,
                    [n] => n
                        .parse()
                        .map_err(|_| format!("{cmd} {n}: not a number of instructions"))?,
                    _ => return Err(format!("{cmd} [N]")),
                };
                self.run_for(n, true, matches!(cmd, "next" | "n"), false)
            }
            "break" | "b" => {
                let c = self.p.place(one("PLACE")?)?;
                self.breaks[c] = true;
                println!("breakpoint at {}", self.name(c));
            }
            "watch" => {
                let c = self.p.place(one("PLACE")?)?;
                self.watches[c] = true;
                println!("watching {}", self.name(c));
            }
            "delete" | "d" => match args {
                [] => {
                    self.breaks = [false; 100];
                    self.watches = [false; 100];
                }
                [a] => {
                    let c = self.p.place(a)?;
                    if !self.breaks[c] && !self.watches[c] {
                        return Err(format!("no breakpoint or watch on {}", self.name(c)));
                    }
                    self.breaks[c] = false;
                    self.watches[c] = false;
                }
                _ => return Err("delete [PLACE]".into()),
            },
            "info" | "i" => match args {
                ["break" | "b" | "breakpoints" | "watch" | "watches"] => {
                    let mut none = true;
                    for c in 0..100 {
                        for (on, kind) in [(self.breaks[c], "breakpoint"), (self.watches[c], "watch")] {
                            if on {
                                println!("{kind:<10} {}", self.p.line(&self.m, c, kind == "breakpoint"));
                                none = false;
                            }
                        }
                    }
                    if none {
                        println!("no breakpoints or watches");
                    }
                }
                ["registers" | "r" | "reg"] => {
                    let cards: Vec<String> = self.m.cards().take(10).map(|c| c.to_string()).collect();
                    let more = if self.m.cards().count() > 10 { " ..." } else { "" };
                    println!(
                        "pc {:02}  acc {:+05}  steps {}  reader [{}{more}]",
                        self.m.pc(),
                        self.m.acc(),
                        self.m.steps(),
                        cards.join(" ")
                    );
                }
                _ => return Err("info break, or info registers".into()),
            },
            "print" | "p" => self.print(one("PLACE")?)?,
            "x" => {
                let (from, n) = match args {
                    [a] => (self.p.place(a)?, 10),
                    [a, n] => (
                        self.p.place(a)?,
                        n.parse().map_err(|_| format!("x {a} {n}: not a number of cells"))?,
                    ),
                    _ => return Err("x PLACE [N]".into()),
                };
                let cells: Vec<usize> = (from..(from + n).min(100)).collect();
                for row in cells.chunks(10) {
                    let words: Vec<String> = row.iter().map(|&c| format!("{:>4}", written(self.m.cell(c)))).collect();
                    println!("{:02}: {}", row[0], words.join(" "));
                }
            }
            "list" | "l" => {
                let at = match args {
                    [] => self.m.pc(),
                    [a] => self.p.place(a)?,
                    _ => return Err("list [PLACE]".into()),
                };
                self.list(at)
            }
            "set" => {
                let [what, v] = args else {
                    return Err("set PLACE VALUE".into());
                };
                if *what == "pc" {
                    let c = self.p.place(v)?;
                    self.m.set_pc(c);
                    self.halted = false;
                    return Ok(());
                }
                let v: i32 = v.parse().map_err(|_| format!("set {what} {v}: not a number"))?;
                match *what {
                    "acc" if (-9999..=9999).contains(&v) => self.m.set_acc(v),
                    "acc" => return Err("the accumulator holds -9999..9999".into()),
                    _ => {
                        let c = self.p.place(what)?;
                        if !(-999..=999).contains(&v) {
                            return Err("a cell holds -999..999".into());
                        }
                        if c == 0 {
                            return Err("cell 00 is wired to 001".into());
                        }
                        self.m.set_cell(c, v);
                    }
                }
            }
            "input" => {
                let cards = values(&args.join(" "))?;
                self.m.push(&cards);
            }
            "help" | "h" => print!("{}", help()),
            "quit" | "q" => {}
            _ => return Err(format!("{cmd}: not a command (help lists them)")),
        }
        Ok(())
    }
}

/// Debug: the commands given with -x, then (unless --batch) each line of standard input until it ends or `quit`,
/// with a prompt when it is a terminal.
fn debug(file: &str, p: &Program, o: &Options) {
    let mut s = Session {
        p,
        m: p.start.clone(),
        breaks: [false; 100],
        watches: [false; 100],
        halted: false,
        limit: o.limit,
        last: None,
        failed: false,
    };
    let tty = std::io::stdin().is_terminal();
    if tty && !o.batch && !o.quiet {
        println!(
            "ekg (cardiaq) {}: {file}. help lists the commands.",
            env!("CARGO_PKG_VERSION")
        );
        println!("{}", p.line(&s.m, s.m.pc(), true));
    }
    let mut going = true;
    for c in &o.script {
        going = s.command(c);
        if !going {
            break;
        }
    }
    if going && !o.batch {
        // read as bytes: a line that isn't UTF-8 is a command not understood, not the end of the session
        let mut stdin = std::io::stdin().lock();
        let mut line = vec![];
        loop {
            if tty {
                print!("(ekg) ");
                let _ = std::io::stdout().flush();
            }
            line.clear();
            match stdin.read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) => die(format!("standard input: {}", getopt::os_error(&e))),
            }
            if !s.command(String::from_utf8_lossy(&line).trim_end_matches(['\n', '\r'])) {
                break;
            }
        }
    }
    std::process::exit((o.batch && s.failed) as i32)
}

/// What a profile counts, cell by cell: the instructions each ran, and the times each was written.
struct Counts {
    runs: [u64; 100],
    writes: [u64; 100],
}

/// Run the program to the end, at most `limit` instructions, counting each instruction run and each cell written;
/// a card from `more` whenever an INP finds the reader empty. Why it stopped.
fn profile_run(m: &mut Machine, limit: u64, more: &mut impl Iterator<Item = i32>) -> (Counts, &'static str) {
    let mut c = Counts {
        runs: [0; 100],
        writes: [0; 100],
    };
    let mut tty = Teletype { profile: true };
    let why = loop {
        if m.steps() >= limit {
            break "still running";
        }
        let pc = m.pc();
        let w = m.cell(pc);
        let before = m.steps();
        let event = m.step(&mut tty);
        if m.steps() > before {
            c.runs[pc] += 1;
            if let Some(t) = target(w).filter(|&t| m.written_at()[t] == m.steps()) {
                c.writes[t] += 1;
            }
        }
        match event {
            Event::Halt => break "halted",
            Event::Input => match more.next() {
                Some(card) => m.push(&[card]),
                None => break "out of cards",
            },
            _ => {}
        }
    };
    (c, why)
}

fn profile(file: &str, p: &Program, limit: u64, stdin: bool) {
    let mut m = p.start.clone();
    let t = Instant::now();
    // the limit is on the program's instructions, the loader's not among them
    let end = limit.saturating_add(p.loading);
    let (c, why) = if stdin {
        profile_run(&mut m, end, &mut cli::stdin_cards("ekg"))
    } else {
        profile_run(&mut m, end, &mut std::iter::empty())
    };
    let dt = t.elapsed().as_secs_f64();
    let _ = std::io::stderr().flush();
    let total: u64 = c.runs.iter().sum();
    let loading = if p.loading > 0 {
        format!(" (and {} loading the deck)", p.loading)
    } else {
        String::new()
    };
    println!(
        "{file}: {why} after {total} instructions{loading}, in {dt:.3} s ({:.1} million a second)",
        (total + p.loading) as f64 / dt.max(1e-9) / 1e6
    );
    let pct = |n: u64| 100.0 * n as f64 / total.max(1) as f64;

    let mut hot: Vec<usize> = (0..100).filter(|&a| c.runs[a] > 0).collect();
    hot.sort_by_key(|&a| (std::cmp::Reverse(c.runs[a]), a));
    println!("\nHottest cells:");
    for &a in hot.iter().take(10) {
        println!("{:>12} {:>6.2}%  {}", c.runs[a], pct(c.runs[a]), p.line(&m, a, true));
    }

    println!("\nThe listing, each cell's runs:");
    let mut cells: Vec<usize> = p.rows.iter().map(|r| r.addr).collect();
    cells.extend(hot.iter().filter(|&&a| p.row(a).is_none()));
    cells.sort();
    for a in cells {
        let runs = match c.runs[a] {
            0 => format!("{:>12} {:>7}", "", ""),
            n => format!("{n:>12} {:>6.2}%", pct(n)),
        };
        println!("{runs}  {}", p.line(&m, a, c.runs[a] > 0));
    }

    let wrote: Vec<usize> = (0..100).filter(|&a| c.writes[a] > 0).collect();
    println!("\nCells written by the program:");
    if wrote.is_empty() {
        println!("  none");
    }
    for a in wrote {
        let what = match (c.runs[a], a) {
            (0, 99) => "the way back JMP leaves",
            (0, _) => "",
            (_, 99) => "the way back JMP leaves, run: a return",
            _ => "run too: code the program rewrites",
        };
        let label = p.label(a).unwrap_or("");
        let line = format!("  {a:02}  {label:<8} written {:>10}  {what}", c.writes[a]);
        println!("{}", line.trim_end());
    }

    let never: Vec<String> = p
        .rows
        .iter()
        .filter(|r| r.op.is_none_or(|o| o != "DATA") && c.runs[r.addr] == 0 && r.contents >= 0)
        .map(|r| p.line(&m, r.addr, true))
        .collect();
    println!("\nListed instructions never run:");
    if never.is_empty() {
        println!("  none");
    }
    for l in never {
        println!("  {l}");
    }
    if why == "still running" {
        die(format!(
            "still running after {limit} instructions: stopped (--limit N to run longer)"
        ))
    }
}

/// The command line.
struct Options {
    file: Option<String>,
    input: Vec<String>,
    script: Vec<String>,
    profile: bool,
    batch: bool,
    quiet: bool,
    limit: u64,
    start: Option<usize>,
}

/// The options: `-x` and `-q` have letters too.
const OPTS: [getopt::Opt; 9] = [
    getopt::Opt {
        short: Some('x'),
        long: "eval-command",
        arg: true,
    },
    getopt::Opt {
        short: None,
        long: "batch",
        arg: false,
    },
    getopt::Opt {
        short: Some('q'),
        long: "quiet",
        arg: false,
    },
    getopt::Opt {
        short: None,
        long: "silent",
        arg: false,
    },
    getopt::Opt {
        short: None,
        long: "profile",
        arg: false,
    },
    getopt::Opt {
        short: None,
        long: "limit",
        arg: true,
    },
    getopt::Opt {
        short: None,
        long: "start-at",
        arg: true,
    },
    getopt::Opt {
        short: None,
        long: "help",
        arg: false,
    },
    getopt::Opt {
        short: None,
        long: "version",
        arg: false,
    },
];

/// Read the command line as GNU getopt_long does (`getopt`, as burn and carc read theirs): options anywhere among
/// the operands, every argument after `--` an operand. The first operand is the file, the rest the input.
fn options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        file: None,
        input: vec![],
        script: vec![],
        profile: false,
        batch: false,
        quiet: false,
        limit: LIMIT,
        start: None,
    };
    let (given, operands) = getopt::try_parse(&OPTS, args.iter().cloned(), false)?;
    for (name, value) in given {
        o.set(name, value)?;
    }
    let mut operands = operands.into_iter();
    o.file = operands.next();
    o.input = operands.collect();
    Ok(o)
}

impl Options {
    fn set(&mut self, name: &str, value: Option<String>) -> Result<(), String> {
        let v = value.unwrap_or_default();
        match name {
            "eval-command" => self.script.push(v),
            "batch" => self.batch = true,
            "quiet" | "silent" => self.quiet = true,
            "profile" => self.profile = true,
            "limit" => {
                self.limit = v
                    .parse()
                    .ok()
                    .filter(|&n| n > 0)
                    .ok_or(format!("invalid --limit '{v}': a number of instructions"))?
            }
            "start-at" => {
                let cell = v.parse().ok().filter(|&c: &usize| c < 100 && v.len() <= 2);
                self.start = Some(cell.ok_or(format!("invalid --start-at '{v}': a cell, 00 to 99"))?)
            }
            "help" => {
                print!("{}\n{}", usage(), help());
                std::process::exit(0)
            }
            "version" => {
                getopt::version("ekg");
                std::process::exit(0)
            }
            _ => unreachable!("an option in OPTS"),
        }
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let o = options(&args).unwrap_or_else(|e| bad_usage(&e));
    let Some(file) = o.file.as_deref() else {
        bad_usage("no program: give it a FILE")
    };
    if o.profile && !o.script.is_empty() {
        bad_usage("--profile carries out no commands (-x)")
    }
    let input = values(&o.input.join(" ")).unwrap_or_else(|e| bad_usage(&e));
    let p = Program::load(file, o.start, &input);
    if o.profile {
        profile(file, &p, o.limit, o.input.is_empty())
    } else {
        debug(file, &p, &o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(text: &str) -> Program {
        let asm = assemble::assemble(text);
        let mut m = Machine::new();
        m.load_cells(&asm.rows.iter().map(|r| (r.addr, r.contents)).collect::<Vec<_>>());
        m.set_pc(asm.start.unwrap());
        Program::new(m, 0, asm.rows)
    }

    #[test]
    fn a_place_is_a_cell_a_label_or_a_label_plus_n() {
        let p = program("10 top CLA n\n11 HRS\n98 n DATA 5\n");
        assert_eq!(p.place("7"), Ok(7));
        assert_eq!(p.place("07"), Ok(7));
        assert_eq!(p.place("top"), Ok(10));
        assert_eq!(p.place("top+1"), Ok(11));
        assert!(p.place("n+2").is_err());
        assert!(p.place("100").is_err());
        assert!(p.place("nowhere").is_err());
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_command_line_is_read_as_getopt_long_reads_it() {
        let o = options(&args(&[
            "-q",
            "f.cq",
            "-xrun",
            "--eval-command=p n",
            "-x",
            "c",
            "--prof",
            "--lim=50",
            "--start-at",
            "10",
            "--",
            "-5",
            "7",
        ]))
        .unwrap();
        assert_eq!(o.file.as_deref(), Some("f.cq"));
        assert_eq!(o.script, ["run", "p n", "c"]);
        assert_eq!(o.input, ["-5", "7"]);
        assert!(o.quiet && o.profile && !o.batch);
        assert_eq!((o.limit, o.start), (50, Some(10)));
        assert!(options(&args(&["-qxrun"])).unwrap().script == ["run"]);
        let err = |a: &[&str]| options(&args(a)).err().unwrap();
        assert_eq!(err(&["--frob"]), "unrecognized option '--frob'");
        assert_eq!(err(&["-z"]), "invalid option -- 'z'");
        assert_eq!(err(&["-x"]), "option requires an argument -- 'x'");
        assert_eq!(err(&["--limit"]), "option '--limit' requires an argument");
        assert_eq!(err(&["--batch=1"]), "option '--batch' doesn't allow an argument");
        assert_eq!(
            err(&["--s"]),
            "option '--s' is ambiguous; possibilities: '--silent' '--start-at'"
        );
        assert!(err(&["--start-at=100"]).starts_with("invalid --start-at"));
    }

    #[test]
    fn what_an_instruction_writes() {
        assert_eq!(target(34), Some(34)); // INP 34
        assert_eq!(target(0), None); // INP 00 reads a card into nothing
        assert_eq!(target(612), Some(12)); // STO 12
        assert_eq!(target(600), None); // STO 00: cell 00 is wired
        assert_eq!(target(810), Some(99)); // JMP leaves the way back in 99
        assert_eq!(target(-612), None); // skipped
        assert_eq!(target(112), None);
    }

    #[test]
    fn a_line_is_the_listings_until_the_program_writes_the_cell() {
        let p = program("10 top CLA n  load it\n11 x DATA 0   becomes an instruction\n98 n DATA 5\n");
        let mut m = p.start.clone();
        assert_eq!(p.line(&m, 10, true), "10   198  top      CLA  n        ; load it");
        assert_eq!(p.line(&m, 98, false), "98   005  n        DATA 5");
        m.set_cell(11, 598);
        assert_eq!(
            p.line(&m, 11, true),
            "11   598  x        OUT  n        ; becomes an instruction (listed 000)"
        );
        assert_eq!(
            p.line(&m, 11, false),
            "11   598  x        DATA 598      ; becomes an instruction"
        );
    }
}
