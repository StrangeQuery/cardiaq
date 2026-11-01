# HEARTBURN

A Rust implementation of the classic cardiac cardboard computer with one goal: SPEED.

CARDIAC (the CARDboard Illustrative Aid to Computation, by David Hagelbarger and Saul Fingerman) was a computer you
worked by hand: 100 cells of three decimal digits, a four-digit accumulator, ten instructions, and a paper bug you
slid along as the program counter. A person running it manages an instruction every few seconds. HEARTBURN runs
standard CARDIAC, exactly as the manual and Brian Stuart's simulator define it, as fast as it can possibly be made
to go. That is the premise: nothing a program can see differs from the cardboard machine, and everything it can't
see is tuned.

[Install](#install) • [Quick start](#quick-start) • [Decks](#decks) • [carc](#carc) • [ekg](#ekg) • [How fast](#how-fast) •
[As a library](#as-a-library)

## Install

    cargo install heartburn

This installs one command, `burn`.

## Quick start

    burn add.deck -- 123 456      # input after --
    echo 123 456 | burn add.deck  # or on standard input, each card read when an INP asks for it
    burn bench                    # how fast this machine runs CARDIAC

`burn` runs a deck and prints each card the program punches, on a line of its own, as it is punched. How it stopped,
and after how many instructions, goes to standard error, so the output pipes cleanly:

    $ burn add.deck -- 123 456
    579
         halted after 34 instructions

Cards are always printed as numbers. For a program whose cards are characters (65 for A, 10 for a newline), pipe the
output through perl:

    burn hello.deck | perl -CO -ne 'print chr if $_ >= 0'

That prints each card as the Unicode character with that number and skips negative cards. (`awk '{printf "%c", $1}'`
also works, for 0-127 only.)

## Decks

`burn` runs decks and nothing else. A deck is a program as CARDIAC loads it: plain text, one card per line (lines
starting with `#` are comments): the two-card loader, 002 and 800; an address card and a value card for each cell;
then the pair 002 8xx that starts the program at xx. Any CARDIAC simulator can load one. `burn` knows a deck by what
is in it, the loader's cards first, not by its name; by convention it is `NAME.deck`.

To make a deck, write the program as a listing and assemble it with [`carc`](#carc), or write it in
[CARDIAC Code](https://lab.strangequery.com/cardiac/code/), the CARDIAC IDE in your browser, and press Export:
`NAME.cardiac` becomes `NAME.deck`.

A program too big for memory comes in several decks: `NAME.deck`, `NAME.2.deck`, and so on. A deck stops with the
PC on the number of the deck to load next (HRS n; on 00 it is done), and the operator, here `burn`, puts that deck in
the reader and starts the loader at 00: any deck, in any order, as often as asked.

## carc

`carc` is the assembler: it makes a listing into a deck. A listing has a cell a line, in Brian Stuart's six
columns, `ADDRESS CONTENTS LABEL OPCODE OPERAND COMMENT`, or the manual's `ADDRESS CONTENTS COMMENT`; the contents
may be left out, and are then worked out from the opcode and the operand (a number, `label` or `label+N`). Lines
that aren't cells (titles, prose, `;` and `#` comments) are skipped.

    carc count.cardiac                # writes count.deck beside it
    carc count.cardiac -o out.deck --start 10

The deck starts at `--start`, or else where the listing says: a `Start at NN.` line, the line labelled `start`, its
first instruction. What is wrong is reported the way a compiler reports it, and no deck is written while there are
errors:

    count.cardiac:4:8: error: unknown label nowhere
    count.cardiac:6:4: warning: contents +199, but CLA 20 is +120: loading the contents

Contents that disagree with the opcode and operand are a warning: the contents are what load. CARDIAC Code reads
listings with the same assembler (`heartburn::assemble`), so a listing assembles the same in both.

## ekg

`ekg` is the debugger and the profiler. It takes a listing (assembled as `carc` assembles it, its labels and
comments kept) or a deck (loaded by its own loader, as `burn` loads it, and disassembled), told apart by what is in
it, and the program's input after it:

    ekg mul.cardiac -- 6 7                 # debug: commands on standard input
    ekg --batch mul.cardiac -x 'break 17' -x run -x 'p p'
    ekg --profile mul.cardiac -- 6 7       # run it to the end and report where the instructions went

The commands are gdb's where gdb has one: `run` (`r`), `continue` (`c`), `step` (`s`) and `next` (`n`) with a
count, `break` (`b`), `watch`, `delete` (`d`), `info break`, `info registers` (`i r`), `print` (`p`), `x`, `list`
(`l`), `set`, `quit` (`q`), `help` (`h`); and `input CARDS`, which puts cards in the reader. A place is a cell, a
label or `label+N`. A breakpoint stops before its cell's instruction runs; a watch stops after an instruction
writes its cell, which is how to catch a program rewriting its own code. `next` runs a routine a JMP calls through,
to the cell after the JMP. An empty line repeats the last step, next or continue. Each stop shows the next
instruction as the listing has it, decoded afresh where the program has rewritten the cell; what the program
punches is printed as it is punched, each card on a line starting `out:`; an INP with no card left stops and waits
for `input`. Commands given with `-x` (`--eval-command`) run first; with `--batch` ekg then exits, else it reads
more from standard input.

    $ ekg --batch mul.cardiac -x 'break 17' -x run -x 'watch p' -x c -x 'info r' -- 6 7
    breakpoint at 17
    breakpoint, cell 17
    17   230           ADD  a
    watching 32 (p)
    watch, cell 32 (p): 000 -> 006, written by cell 18
    19   812           JMP  loop
    pc 19  acc +0006  steps 9  reader []

`--profile` runs the program to the end and prints a report on standard output, the program's cards going to
standard error: how many instructions it ran, the hottest cells, every cell of the listing with its runs, the cells
the program wrote (and which of those it ran too: code it rewrites), and the listed instructions it never ran.
With no input given, its cards are read from standard input, as `burn` reads them.

    $ ekg --profile mul.cardiac -- 6 7 2>/dev/null
    mul.cardiac: halted after 63 instructions, in 0.000 s (7.8 million a second)

    Hottest cells:
               8  12.70%  12   131  loop     CLA  b        ; b goes down to 0
               8  12.70%  13   733           SUB  one
               ...

    The listing, each cell's runs:
               1   1.59%  10   030  start    INP  a
               1   1.59%  11   031           INP  b
               8  12.70%  12   131  loop     CLA  b        ; b goes down to 0
               ...
               1   1.59%  21   900           HRS  00
                          22   810           JMP  start    ; never run
                          30   006  a        DATA 6
               ...

    Cells written by the program:
      30  a        written          1
      31  b        written          8
      32  p        written          7
      99           written          7  the way back JMP leaves

    Listed instructions never run:
      22   810           JMP  start    ; never run

A run stops at 1,000,000,000 instructions unless `--limit=N` says otherwise; `--start=NN` starts a listing
somewhere else. ekg drives the machine an instruction at a time to see each one: it profiles about 270 million
instructions a second, a quarter of `burn`'s speed, and `burn` pays nothing for it. `ekg --help` has the rest.

## How fast

`burn bench` runs three countdown loops, one inside another (626.5 million instructions), on one core. On an Apple
M5 Ultra, the same program on the CARDIAC emulators that could be built and run here (October 2026):

| emulator                          | language   | million instructions a second | per instruction |
|-----------------------------------|------------|-------------------------------|-----------------|
| **HEARTBURN**                     | Rust       | **1,070**                     | **0.94 ns**     |
| InPermutation/CARDIAC `cem.c`     | C (`-O2`)  | 438                           | 2.3 ns          |
| fzaca/CardiacVisualizer           | Rust       | 110                           | 9.1 ns          |
| weibell/python-cardiac-emu        | Python 3.9 | 2.4                           | 417 ns          |
| rzinkstok/cardiac                 | Python 3.9 | 1.25                          | 800 ns          |

That is per core: a CARDIAC program is one instruction after another, so one run uses one core, and separate runs
go side by side on as many cores as the machine has.

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

`wasm/` builds HEARTBURN for a web page: the `Machine` and the assembler behind a plain C API, no dependencies. It
is the engine inside CARDIAC Code, and its assembler.

    rustup target add wasm32-unknown-unknown    # once
    cd wasm && cargo build --release --lib --target wasm32-unknown-unknown

gives `wasm/target/wasm32-unknown-unknown/release/heartburn_wasm.wasm`, about 90 KB. `wasm/test/compare.py` checks
it against native HEARTBURN, session by session, in a browser.

## License

MIT.
