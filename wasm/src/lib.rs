//! HEARTBURN as WebAssembly: one `cardiac::Machine`, held by a web page, behind a plain C API over linear memory.
//! No bindings generator: the page passes cards through a buffer it asks for (`hb_buffer`), and reads memory, the
//! cells' owners and the cards punched through pointers into this module's memory.
//!
//! Every call checks what it is given and says so (-1) rather than trapping.
//!
//! Stops (`hb_run`, `hb_step`): 0 the steps asked for have run, 1 halted (HRS: the PC on its address), 2 at a
//! breakpoint, 3 an INP waiting for a card, 100 + n tape n put in the reader (HRS n), the PC on 00.
//!
//! The assembler (`heartburn::assemble`) is here too, for the page to read its listings with: it passes text,
//! UTF-8, through `hb_text`, and gets back JSON (`hb_assemble`), a deck (`hb_deck`), or one line's columns
//! (`hb_tokenize`). Offsets in what comes back are in UTF-16 units, as the page counts them. Its JSON is written by
//! hand, without `format!`, to keep the module small.
use std::cell::RefCell;

use heartburn::assemble::{self, Line};
use heartburn::cardiac::{Event, Machine};

#[derive(Default)]
struct State {
    machine: Machine,
    /// what the page passes in
    buffer: Vec<i32>,
    tapes: Vec<Vec<i32>>,
    punched: Vec<i32>,
    /// the text the page passes in, UTF-8: a listing, or a line of one
    text: Vec<u8>,
    /// what the assembler gives back: JSON, or a deck
    out: String,
    /// a line's columns (`hb_tokenize`)
    columns: [i32; 12],
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

/// A buffer of `n` bytes for the page to put text in, UTF-8; its address. Each call that takes `n` reads its first
/// `n`.
#[no_mangle]
pub extern "C" fn hb_text(n: u32) -> *mut u8 {
    with(|s| {
        s.text.clear();
        s.text.resize(n as usize, 0);
        s.text.as_mut_ptr()
    })
}

/// What the assembler last gave back (`hb_assemble`, `hb_deck`): its address.
#[no_mangle]
pub extern "C" fn hb_out() -> *const u8 {
    with(|s| s.out.as_ptr())
}

fn text(s: &State, n: u32) -> Option<&str> {
    std::str::from_utf8(s.text.get(..n as usize)?).ok()
}

/// A string as JSON: quoted, and escaped.
fn json(out: &mut String, v: Option<&str>) {
    let Some(v) = v else {
        out.push_str("null");
        return;
    };
    out.push('"');
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                out.push_str(if (c as u32) < 0x10 { "\\u000" } else { "\\u001" });
                out.push(char::from(b"0123456789abcdef"[c as usize & 15]));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn number(out: &mut String, v: Option<i64>) {
    match v {
        Some(v) => assemble::decimal(out, v, 1),
        None => out.push_str("null"),
    }
}

/// Assemble the text's first `n` bytes, a listing: JSON at `hb_out`, its length in bytes; -1 if it isn't UTF-8.
///
/// `{"lines": N, "start": S, "rows": [{"ln", "addr", "contents", "label", "op", "operand", "comment"}],
/// "diagnostics": [{"ln", "s", "e", "msg", "warn", "dup", "want"}], "words": [W, ...]}`: lines from 0, a
/// diagnostic's span (s to e) in UTF-16 units in its line, null for what isn't there; `words` has what each line's
/// opcode and operand make.
#[no_mangle]
pub extern "C" fn hb_assemble(n: u32) -> i32 {
    with(|s| {
        let Some(t) = text(s, n) else {
            return -1;
        };
        let asm = assemble::assemble(t);
        let lines = assemble::lines(t);
        let mut o = String::from("{\"lines\":");
        number(&mut o, Some(asm.lines as i64));
        o.push_str(",\"start\":");
        number(&mut o, asm.start.map(|a| a as i64));
        o.push_str(",\"rows\":[");
        for (k, r) in asm.rows.iter().enumerate() {
            o.push_str(if k > 0 { ",{\"ln\":" } else { "{\"ln\":" });
            number(&mut o, Some(r.line as i64));
            o.push_str(",\"addr\":");
            number(&mut o, Some(r.addr as i64));
            o.push_str(",\"contents\":");
            number(&mut o, Some(r.contents.into()));
            o.push_str(",\"label\":");
            json(&mut o, r.label.as_deref());
            o.push_str(",\"op\":");
            json(&mut o, r.op);
            o.push_str(",\"operand\":");
            json(&mut o, r.operand.as_deref());
            o.push_str(",\"comment\":");
            json(&mut o, Some(&r.comment));
            o.push('}');
        }
        o.push_str("],\"diagnostics\":[");
        for (k, d) in asm.diagnostics.iter().enumerate() {
            let line = lines.get(d.line).copied().unwrap_or_default();
            o.push_str(if k > 0 { ",{\"ln\":" } else { "{\"ln\":" });
            number(&mut o, Some(d.line as i64));
            o.push_str(",\"s\":");
            number(&mut o, Some(assemble::utf16(line, d.span.start) as i64));
            o.push_str(",\"e\":");
            number(&mut o, Some(assemble::utf16(line, d.span.end) as i64));
            o.push_str(",\"msg\":");
            json(&mut o, Some(&d.message));
            o.push_str(if d.warning { ",\"warn\":true" } else { ",\"warn\":false" });
            o.push_str(if d.duplicate { ",\"dup\":true" } else { ",\"dup\":false" });
            o.push_str(",\"want\":");
            number(&mut o, d.want.map(i64::from));
            o.push('}');
        }
        o.push_str("],\"words\":[");
        for (k, w) in asm.words.iter().enumerate() {
            if k > 0 {
                o.push(',');
            }
            number(&mut o, w.map(i64::from));
        }
        o.push_str("]}");
        s.out = o;
        s.out.len() as i32
    })
}

/// The deck for the text's first `n` bytes, a listing, starting at `start`: its cards, one a line (as a deck file
/// has them), at `hb_out`; their length in bytes. -1 if the text isn't UTF-8 or the start isn't a cell.
#[no_mangle]
pub extern "C" fn hb_deck(n: u32, start: u32) -> i32 {
    if start >= 100 {
        return -1;
    }
    with(|s| {
        let Some(t) = text(s, n) else {
            return -1;
        };
        s.out = assemble::deck_text(&assemble::deck(&assemble::assemble(t), start as usize));
        s.out.len() as i32
    })
}

/// One line's columns, the text's first `n` bytes: 12 words, in UTF-16 units, -1 for what isn't there. The first
/// is what the line is (0 blank, 1 a comment, 2 prose, 3 a cell; -1 if it isn't UTF-8); a comment or prose starts
/// at the second. A cell: the address, contents, label, opcode and operand, each its start and end, then where the
/// comment starts.
#[no_mangle]
pub extern "C" fn hb_tokenize(n: u32) -> *const i32 {
    with(|s| {
        let mut c = [-1; 12];
        if let Some(t) = text(s, n) {
            let at = |b: usize| assemble::utf16(t, b) as i32;
            match assemble::tokenize(t) {
                Line::Blank => c[0] = 0,
                Line::Comment(from) => c[..2].copy_from_slice(&[1, at(from)]),
                Line::Prose(from) => c[..2].copy_from_slice(&[2, at(from)]),
                Line::Cell(cell) => {
                    c[0] = 3;
                    let spans = [Some(cell.address), cell.contents, cell.label, cell.opcode, cell.operand];
                    for (k, r) in spans.iter().enumerate() {
                        if let Some(r) = r {
                            c[1 + 2 * k] = at(r.start);
                            c[2 + 2 * k] = at(r.end);
                        }
                    }
                    if let Some(from) = cell.comment {
                        c[11] = at(from);
                    }
                }
            }
        }
        s.columns = c;
        s.columns.as_ptr()
    })
}
