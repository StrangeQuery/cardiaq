//! HEARTBURN as WebAssembly: one `cardiac::Machine`, held by a web page, behind a plain C API over linear memory.
//! No bindings generator: the page passes cards through a buffer it asks for (`hb_buffer`), and reads memory, the
//! cells' owners and the cards punched through pointers into this module's memory.
//!
//! Every call checks what it is given and says so (-1) rather than trapping.
//!
//! Stops (`hb_run`, `hb_step`): 0 the steps asked for have run, 1 halted (HRS: the PC on its address), 2 at a
//! breakpoint, 3 an INP waiting for a card, 100 + n tape n put in the reader (HRS n), the PC on 00.
use std::cell::RefCell;

use heartburn::cardiac::{Event, Machine};

#[derive(Default)]
struct State {
    machine: Machine,
    /// what the page passes in
    buffer: Vec<i32>,
    tapes: Vec<Vec<i32>>,
    punched: Vec<i32>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with<T>(f: impl FnOnce(&mut State) -> T) -> T {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn card(w: i32) -> bool {
    (-999..=999).contains(&w)
}

fn code(e: Event) -> i32 {
    match e {
        Event::Budget => 0,
        Event::Halt => 1,
        Event::Break => 2,
        Event::Input => 3,
        Event::Tape(n) => 100 + n as i32,
    }
}

/// A buffer of `n` words for the page to fill; its address. Each call that takes `n` reads its first `n`.
#[no_mangle]
pub extern "C" fn hb_buffer(n: u32) -> *mut i32 {
    with(|s| {
        s.buffer.clear();
        s.buffer.resize(n as usize, 0);
        s.buffer.as_mut_ptr()
    })
}

/// A cleared CARDIAC: memory, accumulator, PC 00, reader, tapes, breakpoints and cards punched.
#[no_mangle]
pub extern "C" fn hb_reset() {
    with(|s| {
        s.machine = Machine::new();
        s.tapes.clear();
        s.punched.clear();
    })
}

/// The buffer's first `n` cards are the program's next tape (tape 1, then 2, ...): the tapes HRS n feeds. Tape 1
/// isn't put in the reader by this: `hb_push` it. -1 if a card isn't one.
#[no_mangle]
pub extern "C" fn hb_add_tape(n: u32) -> i32 {
    with(|s| {
        let Some(t) = s.buffer.get(..n as usize).filter(|t| t.iter().all(|&c| card(c))) else {
            return -1;
        };
        s.tapes.push(t.to_vec());
        let tapes = s.tapes.clone();
        s.machine.set_tapes(tapes);
        0
    })
}

/// Put the buffer's first `n` cards in the reader, after those in it. -1 if a card isn't one.
#[no_mangle]
pub extern "C" fn hb_push(n: u32) -> i32 {
    with(|s| {
        let Some(c) = s.buffer.get(..n as usize).filter(|t| t.iter().all(|&c| card(c))) else {
            return -1;
        };
        s.machine.push(c);
        0
    })
}

/// Write cells straight into memory, tape 1's: the buffer's first `n` words are address and contents pairs.
#[no_mangle]
pub extern "C" fn hb_load_cells(n: u32) -> i32 {
    with(|s| {
        let Some(b) = s.buffer.get(..n as usize).filter(|b| b.len() % 2 == 0) else {
            return -1;
        };
        let cells: Vec<(usize, i32)> = b.chunks(2).map(|p| (p[0] as usize, p[1])).collect();
        if cells.iter().any(|&(a, w)| a >= 100 || !card(w)) {
            return -1;
        }
        s.machine.load_cells(&cells);
        0
    })
}

#[no_mangle]
pub extern "C" fn hb_set_cell(cell: u32, word: i32) -> i32 {
    if cell >= 100 || !card(word) {
        return -1;
    }
    with(|s| s.machine.set_cell(cell as usize, word));
    0
}

#[no_mangle]
pub extern "C" fn hb_set_pc(cell: u32) -> i32 {
    if cell >= 100 {
        return -1;
    }
    with(|s| s.machine.set_pc(cell as usize));
    0
}

#[no_mangle]
pub extern "C" fn hb_set_acc(acc: i32) -> i32 {
    if !(-9999..=9999).contains(&acc) {
        return -1;
    }
    with(|s| s.machine.set_acc(acc));
    0
}

#[no_mangle]
pub extern "C" fn hb_set_break(cell: u32) -> i32 {
    if cell >= 100 {
        return -1;
    }
    with(|s| s.machine.set_break(cell as usize));
    0
}

#[no_mangle]
pub extern "C" fn hb_clear_break(cell: u32) -> i32 {
    if cell >= 100 {
        return -1;
    }
    with(|s| s.machine.clear_break(cell as usize));
    0
}

/// Run until a stop, at most `budget` instructions (a whole number, up to 2^53).
#[no_mangle]
pub extern "C" fn hb_run(budget: f64) -> i32 {
    if budget.is_nan() || budget < 0.0 {
        return -1;
    }
    with(|s| {
        let State { machine, punched, .. } = s;
        code(machine.run(budget as u64, punched))
    })
}

/// Run one instruction (a breakpoint on it doesn't stop it).
#[no_mangle]
pub extern "C" fn hb_step() -> i32 {
    with(|s| {
        let State { machine, punched, .. } = s;
        code(machine.step(punched))
    })
}

/// The 100 cells.
#[no_mangle]
pub extern "C" fn hb_memory() -> *const i32 {
    with(|s| s.machine.memory().as_ptr())
}

/// The tape that last wrote each cell, 100 bytes: 1, 2, ..., or 0 for none.
#[no_mangle]
pub extern "C" fn hb_owners() -> *const u8 {
    with(|s| s.machine.owners().as_ptr())
}

/// The step each cell was last written at, 100 u64s (0 for none): the last step's writes hold `hb_steps()`.
#[no_mangle]
pub extern "C" fn hb_written_at() -> *const u64 {
    with(|s| s.machine.written_at().as_ptr())
}

#[no_mangle]
pub extern "C" fn hb_acc() -> i32 {
    with(|s| s.machine.acc())
}

#[no_mangle]
pub extern "C" fn hb_pc() -> u32 {
    with(|s| s.machine.pc() as u32)
}

/// The instructions run so far.
#[no_mangle]
pub extern "C" fn hb_steps() -> f64 {
    with(|s| s.machine.steps() as f64)
}

/// The phase running: 1, or the number of the last tape fed.
#[no_mangle]
pub extern "C" fn hb_phase() -> u32 {
    with(|s| s.machine.phase() as u32)
}

/// The last three cells run, the last last: written into the buffer, its first three words.
#[no_mangle]
pub extern "C" fn hb_trail() -> *const i32 {
    with(|s| {
        s.buffer.clear();
        s.buffer.extend(s.machine.trail().iter().map(|&c| c as i32));
        s.buffer.as_ptr()
    })
}

/// The cards in the reader, the next first, copied into the buffer; how many.
#[no_mangle]
pub extern "C" fn hb_reader() -> u32 {
    with(|s| {
        s.buffer.clear();
        let cards: Vec<i32> = s.machine.cards().collect();
        s.buffer.extend(cards);
        s.buffer.len() as u32
    })
}

/// The buffer's address (after `hb_reader` or `hb_trail`: what they wrote).
#[no_mangle]
pub extern "C" fn hb_buffer_ptr() -> *const i32 {
    with(|s| s.buffer.as_ptr())
}

/// The cards punched since the last `hb_take_punched`: how many; `hb_punched` is where they are.
#[no_mangle]
pub extern "C" fn hb_punched_len() -> u32 {
    with(|s| s.punched.len() as u32)
}

#[no_mangle]
pub extern "C" fn hb_punched() -> *const i32 {
    with(|s| s.punched.as_ptr())
}

/// Forget the cards punched (once the page has read them).
#[no_mangle]
pub extern "C" fn hb_take_punched() {
    with(|s| s.punched.clear())
}

/// How many cards are in the reader.
#[no_mangle]
pub extern "C" fn hb_reader_len() -> u32 {
    with(|s| s.machine.cards().count() as u32)
}

/// The cells run since the marks were cleared, 100 bytes (1 or 0).
#[no_mangle]
pub extern "C" fn hb_ran() -> *const bool {
    with(|s| s.machine.ran().as_ptr())
}

/// The cells an instruction used as data since the marks were cleared, 100 bytes: CLA, ADD, SUB, OUT, STO and
/// INP's operands, and 99 for JMP.
#[no_mangle]
pub extern "C" fn hb_used() -> *const bool {
    with(|s| s.machine.used().as_ptr())
}

#[no_mangle]
pub extern "C" fn hb_clear_marks() {
    with(|s| s.machine.clear_marks())
}

/// The last instruction run: its cell, its word then, and the accumulator before it ran.
#[no_mangle]
pub extern "C" fn hb_last_cell() -> u32 {
    with(|s| s.machine.last().0 as u32)
}

#[no_mangle]
pub extern "C" fn hb_last_word() -> i32 {
    with(|s| s.machine.last().1)
}

#[no_mangle]
pub extern "C" fn hb_last_acc() -> i32 {
    with(|s| s.machine.last().2)
}
