# cardiaq

Tools for CARDIAC, the 1968 Bell Labs cardboard computer: an assembler, a runtime and a debugger, made to work the way
gcc and gdb do.

CARDIAC (the CARDboard Illustrative Aid to Computation, by David Hagelbarger and Saul Fingerman) was a computer you
worked by hand: 100 cells of three decimal digits, a four-digit accumulator, ten instructions, and a paper bug you
slid along as the program counter. It was made to teach how a computer works, and it still does: the whole machine
fits on one sheet, and every trick a real machine needs (loops, subroutines, arrays, a loader) has to be built from
those ten instructions in plain sight.

cardiaq is the toolkit for writing CARDIAC programs on a real computer. Its engine, HEARTBURN, runs standard CARDIAC
exactly as the manual and Brian Stuart's simulator define it, about a billion instructions a second.

[What you can do](#what-you-can-do) • [What's in it](#whats-in-it) • [Install](#install) •
[Walkthrough](#walkthrough) • [CARDIAC in one page](#cardiac-in-one-page) • [carc](#carc) • [burn](#burn) •
[ekg](#ekg) • [How fast](#how-fast) • [As a library](#as-a-library) • [WebAssembly](#webassembly)

## What you can do

- **Write CARDIAC programs** as listings, the six columns of Brian Stuart's CARDIAC page or the 1968 manual's own
  tables, and assemble them into decks: the cards CARDIAC loads, which any CARDIAC simulator can run.
- **Run them**, from the command line or in a pipeline, with their input from the command line or standard input.
  A program too big for the 100 cells runs as several decks, each loading the next.
- **Debug them** with breakpoints by label, single steps, and watches that catch a program rewriting its own code,
  which CARDIAC programs do all the time.
- **Profile them**: where the instructions went, the hottest loop, the code that never ran.
- **Build on them**: the engine and the assembler are a Rust library, and a WebAssembly module for a web page.
  [Cardigan](https://lab.strangequery.com/cardigan/), the CARDIAC IDE in your browser, is built on them.

## What's in it

| | | like |
|---|---|---|
| [`carc`](#carc) | the assembler: a listing (`NAME.cardiac`) into a deck (`NAME.deck`) | `gcc`, `as` |
| [`burn`](#burn) | the runtime: runs a deck | `java`, `dotnet` |
| [`ekg`](#ekg) | the debugger and profiler: a listing or a deck | `gdb`, `perf` |
| [`heartburn`](#as-a-library) | the Rust library under all three: the machine, the assembler, decks | |
| [`wasm/`](#webassembly) | the machine and the assembler as WebAssembly, for a web page | |

The three commands follow the GNU conventions: `--help` and `--version`, short and long options, `--` to end them,
`-` for standard input, errors as `PROG: message` or `FILE:LINE:COLUMN: error: message`, and exit status 0 for
success, 1 for failure, 2 for a mistake on the command line.

## Install

Prebuilt `burn`, `carc` and `ekg` for macOS, Linux and Windows come with the first release, through Homebrew,
winget and the [releases](https://github.com/StrangeQuery/cardiaq/releases). Until then, build them from source,
with [Rust](https://rustup.rs) installed:

    git clone https://github.com/StrangeQuery/cardiaq
    cd cardiaq
    cargo build --release

The three commands are then in `target/release/`; put them on your PATH.

## Walkthrough

A program that counts down from 5. Save it as `countdown.cardiac`:

    ; Countdown: prints 5, 4, 3, 2, 1, 0 and stops.
    ; Start at 10.
    ; Address Contents Label Opcode Operand Comment
    10   530  start  OUT  n      print n, take one off, and stop once it goes below zero
    11   130         CLA  n
    12   700         SUB  00     00 holds 001: whenever you need a 1, it is there
    13   316         TAC  done
    14   630         STO  n
    15   810         JMP  start
    16   900  done   HRS  00
    30   005  n      DATA 5      counts down from here

Each line is a cell: its address, its contents, a label, the instruction as an opcode and an operand, a comment. The
contents column may be left out; `carc` works it out from the opcode and the operand.

**Assemble it.** `carc` writes `countdown.deck` beside it:

    $ carc countdown.cardiac
    $ cat countdown.deck
    # countdown.deck: countdown.cardiac on cards, for the loader; it starts at 10
    # CARDIAC cards, one per line, read from the top. The program's input goes after them.
    002
    800
    010
    530
    ...
    030
    005
    002
    810

That is the program as CARDIAC loads it: two cards that make a loader, then an address card and a contents card for
each cell, then a jump to the start.

**Run it.**

    $ burn countdown.deck
    5
    4
    3
    2
    1
    0
         halted after 65 instructions

What the program prints goes to standard output; how it stopped goes to standard error, so the output pipes cleanly.
(The 65 includes the loader's instructions, reading the cards in.)

**Make a mistake.** Misspell the label in cell 13 and get cell 11's contents wrong:

    $ carc oops.cardiac
    oops.cardiac:5:6: warning: contents +131, but CLA n is +130: loading the contents
    oops.cardiac:7:23: error: unknown label dun
    carc: oops.cardiac: not assembled: no deck written

**Debug it.** Stop at `start`, look at `n`, watch it change, carry on. Commands can be typed at the `(ekg)` prompt,
or given with `-x`:

    $ ekg --batch countdown.cardiac -x 'break start' -x run -x 'p n' -x 'watch n' -x continue \
          -x continue -x 'info registers' -x delete -x continue
    breakpoint at 10 (start)
    breakpoint, cell 10
    10   530  start    OUT  n        ; print n, take one off, and stop once it goes below zero
    30 (n) = 005
    watching 30 (n)
    out: 5
    watch, cell 30 (n): 005 -> 004, written by cell 14
    15   810           JMP  start
    breakpoint, cell 10
    10   530  start    OUT  n        ; print n, take one off, and stop once it goes below zero
    pc 10  acc +0004  steps 6  reader []
    out: 4
    out: 3
    out: 2
    out: 1
    out: 0
    halted: HRS in cell 16, after 35 instructions

**Profile it.**

    $ ekg --profile countdown.cardiac 2>/dev/null
    countdown.cardiac: halted after 35 instructions, in 0.000 s (1.3 million a second)

    Hottest cells:
               6  17.14%  10   530  start    OUT  n        ; print n, take one off, and stop once it goes below zero
               6  17.14%  11   130           CLA  n
               6  17.14%  12   700           SUB  00       ; 00 holds 001: whenever you need a 1, it is there
               6  17.14%  13   316           TAC  done
               5  14.29%  14   630           STO  n
               5  14.29%  15   810           JMP  start
               1   2.86%  16   900  done     HRS  00
    ...
    Cells written by the program:
      30  n        written          5
      99           written          5  the way back JMP leaves

    Listed instructions never run:
      none

Or do all of it in [Cardigan](https://lab.strangequery.com/cardigan/), in a browser: the same assembler, the same
machine, drawn as it runs.

## CARDIAC in one page

Memory is 100 cells, 00 to 99, each holding a sign and three digits (-999 to 999). The accumulator holds a sign and
four digits. Cell 00 is wired to 001 and can't be changed: that is the loader's first instruction, and a 1 any
program can use. An instruction is three digits: the opcode, then a two-digit address.

| | opcode | |
|---|---|---|
| 0 | `INP a` | read the next input card into cell a |
| 1 | `CLA a` | clear the accumulator and add cell a: load it |
| 2 | `ADD a` | add cell a to the accumulator |
| 3 | `TAC a` | test the accumulator: if it is negative, jump to a |
| 4 | `SFT lr` | shift the accumulator l digits left, then r digits right |
| 5 | `OUT a` | punch cell a on an output card |
| 6 | `STO a` | store the accumulator's last three digits in cell a |
| 7 | `SUB a` | subtract cell a from the accumulator |
| 8 | `JMP a` | jump to a, first writing 8 and the address after the JMP into cell 99: the way back from a subroutine |
| 9 | `HRS a` | halt, with the program counter on a |

There is no index register: a program steps through an array by rewriting the address in one of its own
instructions. That is what `ekg`'s `watch` is for.

## carc

`carc` is the assembler: it makes a listing into a deck.

    carc countdown.cardiac                       # writes countdown.deck beside it
    carc -s 10 -o out.deck countdown.cardiac     # or --start=10 --output=out.deck, before or after the file
    carc - < countdown.cardiac | burn -          # the listing from standard input, the deck to standard output

**Listings.** A listing has a cell a line, in Brian Stuart's six columns, `ADDRESS CONTENTS LABEL OPCODE OPERAND
COMMENT`, or the manual's `ADDRESS CONTENTS COMMENT`. The contents may be left out, and are then worked out from the
opcode and the operand: a number, a `label`, or `label+N`. `DATA` (or `DAT`) puts a number in a cell. Lines that
aren't cells (titles, prose, `;` and `#` comments) are skipped.

**Where it starts.** The deck starts the program at `--start`, or else where the listing says: a `Start at NN.`
line, the line labelled `start`, its first instruction.

**Errors and warnings** are reported the way a compiler reports them, and no deck is written while there are
errors. Contents that disagree with the opcode and operand are a warning: the contents are what load.

| | |
|---|---|
| `-o`, `--output=FILE` | write the deck to FILE (`-` for standard output); by default `NAME.deck` beside `NAME.cardiac`, or standard output for a listing read from standard input |
| `-s`, `--start=NN` | start the program at cell NN |
| `-h`, `--help` | display help and exit |
| `--version` | output version information and exit |

Exit status is 0 if the deck was written, 1 if the listing has errors or a file can't be read or written, 2 for a
mistake on the command line.

## burn

`burn` is the runtime: it runs decks, and nothing else.

    burn countdown.deck              # run it
    burn add.deck -- 123 456         # its input cards after the deck (the -- is optional)
    echo 123 456 | burn add.deck     # or on standard input, each card read when an INP asks for it
    carc - < add.cardiac | burn -    # the deck itself from standard input
    burn bench                       # how fast this machine runs CARDIAC

**Decks.** A deck is plain text, one card per line (lines starting with `#` are comments): the two-card loader, 002
and 800; an address card and a value card for each cell; then the pair 002 8xx that starts the program at xx. `burn`
knows a deck by what is in it, the loader's cards first, not by its name; by convention it is `NAME.deck`.

**Output.** Each card the program punches is printed as a number on a line of its own, as it is punched. How it
stopped, and after how many instructions, goes to standard error. For a program whose cards are characters (65 for
A, 10 for a newline), pipe the output through perl:

    burn hello.deck | perl -CO -ne 'print chr if $_ >= 0'

**Programs in several decks.** A program too big for memory comes as `NAME.deck`, `NAME.2.deck`, and so on. A deck
stops with the PC on the number of the deck to load next (HRS n; on 00 it is done), and `burn`, as the operator
would, puts that deck in the reader and starts the loader at 00: any deck, in any order, as often as asked.

| | |
|---|---|
| `-h`, `--help` | display help and exit |
| `--version` | output version information and exit |

Options go before the deck: everything after it is input, minus signs and all. Exit status is 0 if the program halted
or ran out of cards, 1 if the deck can't be read or the program never stops (a run stops at a billion instructions),
2 for a mistake on the command line.

## ekg

`ekg` is the debugger and the profiler. It takes a listing (assembled as `carc` assembles it, its labels and comments
kept) or a deck (loaded by its own loader, as `burn` loads it, and disassembled), told apart by what is in it, and the
program's input after it:

    ekg countdown.cardiac                          # debug, at the (ekg) prompt
    ekg --batch mul.cardiac -x 'break 17' -x run -x 'p p' -- 6 7
    ekg --profile mul.cardiac -- 6 7               # run it to the end and report where the instructions went

**Debugging.** The commands are gdb's where gdb has one. A PLACE is a cell (00-99), a label, or `label+N`.

| | |
|---|---|
| `run`, `r` | start the program again from the beginning, with its input, and continue |
| `continue`, `c` | run until a breakpoint, a watch, a halt, or an INP with no card |
| `step`, `s` [N] | run one instruction (or N) |
| `next`, `n` [N] | step, but run a routine a JMP calls through, on to the cell after the JMP |
| `break`, `b` PLACE | stop before the instruction in PLACE runs |
| `watch` PLACE | stop after an instruction writes PLACE |
| `delete`, `d` [PLACE] | remove the breakpoint and watch on PLACE (all of them without one) |
| `info break`, `i b` | the breakpoints and watches |
| `info registers`, `i r` | the PC, the accumulator, the instructions run, the cards in the reader |
| `print`, `p` PLACE | a cell's word, and the word as an instruction (also `p acc`, `p pc`) |
| `x` PLACE [N] | the words in N cells from PLACE (10 unless given) |
| `list`, `l` [PLACE] | the listing around the PC (or PLACE), the PC marked `>`, breakpoints `b` |
| `set` PLACE VALUE | write a cell (also `set acc VALUE`, `set pc CELL`) |
| `input` CARDS... | put cards in the reader, after those in it |
| `help`, `h` | the commands |
| `quit`, `q` | leave |

An empty line repeats the last `step`, `next` or `continue`. Each stop shows the next instruction as the listing has
it, decoded afresh where the program has rewritten the cell. What the program punches is printed as it is punched,
each card on a line starting `out:`. An INP with no card left stops and waits for `input`.

**Profiling.** `--profile` runs the program to the end and prints a report on standard output, the program's cards
going to standard error: how many instructions it ran, the hottest cells, every cell of the listing with its runs, the
cells the program wrote (and which of those it ran too: code it rewrites), and the listed instructions it never ran.
With no input given, its cards are read from standard input, as `burn` reads them. ekg drives the machine an
instruction at a time to see each one: it profiles about 270 million instructions a second, a quarter of `burn`'s
speed, and `burn` pays nothing for it.

| | |
|---|---|
| `-x`, `--eval-command=COMMAND` | carry out COMMAND (any number of times, in order) |
| `--batch` | carry out the `-x` commands, then exit, without reading standard input |
| `-q`, `--quiet`, `--silent` | no introduction |
| `--profile` | profile instead of debugging |
| `--limit=N` | stop a run after N instructions (default 1,000,000,000) |
| `--start=NN` | start a listing at cell NN (default: where `carc` would start it) |
| `--help` | display help and exit |
| `--version` | output version information and exit |

Exit status is 0 on success, 1 when something failed (a listing with errors; with `--batch`, a command; in a profile,
a run stopped at the limit), 2 for a usage error.

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

The crate is `heartburn`: the machine (`cardiac`), the assembler `carc` and Cardigan use (`assemble`), decks
(`tape`), and listings in the manual's other forms (`listing`).

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

`wasm/` builds the machine and the assembler for a web page, behind a plain C API, with no dependencies. It is the
engine inside [Cardigan](https://lab.strangequery.com/cardigan/), and its assembler.

    rustup target add wasm32-unknown-unknown    # once
    cd wasm && cargo build --release --lib --target wasm32-unknown-unknown

gives `wasm/target/wasm32-unknown-unknown/release/heartburn_wasm.wasm`, about 68 KB. `wasm/test/compare.py` checks
it against the native machine, session by session, in a browser.

## License

MIT.
