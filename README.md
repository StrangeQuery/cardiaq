# HEARTBURN

A Rust implementation of the classic cardiac cardboard computer with one goal: SPEED.

CARDIAC (the CARDboard Illustrative Aid to Computation, by David Hagelbarger and Saul Fingerman) was a computer you
worked by hand: 100 cells of three decimal digits, a four-digit accumulator, ten instructions, and a paper bug you
slid along as the program counter. A person running it manages an instruction every few seconds. HEARTBURN runs
standard CARDIAC, exactly as the manual and Brian Stuart's simulator define it, as fast as it can possibly be made
to go. That is the premise: nothing a program can see differs from the cardboard machine, and everything it can't
see is tuned.

## How fast

`heartburn bench` runs three countdown loops, one inside another (626.5 million instructions). On an Apple M5 Ultra,
the same program on the CARDIAC emulators that could be built and run here (October 2026):

| emulator                          | language   | million instructions a second | per instruction |
|-----------------------------------|------------|-------------------------------|-----------------|
| **HEARTBURN**                     | Rust       | **1,070**                     | **0.94 ns**     |
| InPermutation/CARDIAC `cem.c`     | C (`-O2`)  | 438                           | 2.3 ns          |
| fzaca/CardiacVisualizer           | Rust       | 110                           | 9.1 ns          |
| weibell/python-cardiac-emu        | Python 3.9 | 2.4                           | 417 ns          |
| rzinkstok/cardiac                 | Python 3.9 | 1.25                          | 800 ns          |

On real programs, 109 compiled CARDIAC machines (4.5 billion instructions, every card checked against the same
programs compiled natively), it runs about 760 million a second: 8 times slower than native Rust doing the
same work, with each CARDIAC instruction doing a fraction of what one native instruction does. (It started at 510
million, 12 times; the first round of tuning is in the history.)

How it gets there, so far: each cell's instruction is decoded once and kept beside its value, decoded again only
when the cell is written (CARDIAC programs rewrite their own code all the time, to index arrays), from a 4 KB table
of every word's decoding, for 100 cells; the accumulator wraps by a compare, not a division; JMP writes cell 99
already decoded; a 101st decoded cell sends the bug round to 00, so no step checks for it; tracing, for saying
where a run stopped, is a compile-time switch that costs nothing when it is off; the loop allocates nothing.
Where it goes next: fewer cycles a step, and the idioms CARDIAC code is made of (an indexed load is four
instructions of self-modifying code) done as one. No JIT: an interpreter, as fast as one can be made to go.

## Install

    cargo install heartburn

## Use

    heartburn run hello.tape            # each card punched, as it is punched
    heartburn run gcd.tape -- 252 105   # input after --, else standard input, read as INP asks
    heartburn run add.txt --start 17 -- 123 456   # a listing, written straight into memory
    heartburn tape prog.lst             # a listing to a tape that loads itself: prog.tape
    heartburn listing prog.tape         # a tape, CARDIASM or any listing in six columns: prog.lst
    heartburn cardiasm prog.cardiasm    # Jason Whittington's CARDIASM to a deck: prog.cardimg
    heartburn bench

A listing is a program as the 1968 manual or Brian Stuart's page prints it: the first two numbers on a line are the
address and the contents, and the rest is comment, so a program copied out of the book runs as it is. Stuart's six
columns (Address, Contents, Label, Opcode, Operand, Comment) work out a cell's contents from its opcode and operand
when they are missing. `--start` says where to start, as the manual does in prose.

A `.tape` is a plain deck, one card per line, that any CARDIAC player loads: the two-card loader, an address and a
value for each cell, and the pair 002 8xx that starts the program. A program too big for memory comes in tapes,
`NAME.tape`, `NAME.2.tape`, and so on. A tape stops with the bug on the number of the tape to load next (HRS n; on 00
it is done), and the operator, here HEARTBURN, puts that tape in the reader and starts the loader at 00: any tape,
in any order, as often as asked.

## As a library

```rust
use heartburn::listing::{self, Listing};
use heartburn::tape::Tape;

// the manual's Program No. 1: read two cards, punch their sum
let read = listing::parse("17 034\n18 035\n19 134\n20 235\n21 636\n22 536\n23 900\n")?;
let Listing::Memory(cells) = read.listing else { return Err("a deck, not a memory listing".into()) };
let tape = Tape::one(listing::deck(17, &cells)?);
let (punched, why, steps) = tape.run(&[2, 3], 1_000)?;   // [5], Halt, 34 instructions
```

`examples/add.rs` is this, runnable: `cargo run --example add`. `cardiac::run` runs a deck directly, and
`cardiac::Machine` is the machine a debugger holds: step it, run it to a breakpoint, read and write it. Breakpoints
cost nothing per instruction, and a plain run doesn't pay for any of it.

## WebAssembly

`wasm/` builds HEARTBURN for a web page: the `Machine` behind a plain C API, no dependencies.

    rustup target add wasm32-unknown-unknown    # once
    cd wasm && cargo build --release --lib --target wasm32-unknown-unknown

gives `wasm/target/wasm32-unknown-unknown/release/heartburn_wasm.wasm`, about 35 KB. `wasm/test/compare.py` checks
it against native HEARTBURN, session by session, in a browser.

## License

MIT.
