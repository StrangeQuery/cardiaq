<p align="center"><img src="assets/cardiaq.png" alt="cardiaq" width="640"></p>

# cardiaq

Modern tools for CARDIAC, the 1968 Bell Labs cardboard computer: an assembler, a debugger, and a _stupid_ fast runtime.

```
carc countdown.cq     # compiles to countdown.cqb
burn countdown.cqb    # runs it in the fast emulator
```

## CARDIAC

[CARDIAC](https://en.wikipedia.org/wiki/CARDboard_Illustrative_Aid_to_Computation)
(the CARDboard Illustrative Aid to Computation, by David Hagelbarger and Saul Fingerman), the classic paper computer.

<p align="center"><img src="assets/cardiac.jpg" alt="A CARDIAC: the cardboard computer, its slides pulled out" width="360"></p>

**cardiaq** is an SDK for writing, debugging, and running CARDIAC programs on contemporary hardware.
Its engine, HEARTBURN, runs CARDIAC at about a billion instructions a second per core on a Mac Studio (Apple M5 Ultra).

[What you can do](#what-you-can-do-with-this-sdk) • [Cardigan](#want-an-ide) • [What's in it](#whats-in-this-repo) •
[Install](#install) • [Walkthrough](#walkthrough) • [CARDIAC in one page](#cardiac-in-one-page) • [carc](#carc) •
[burn](#burn) • [ekg](#ekg) • [How fast](#how-fast) • [As a library](#as-a-library)

## What you can do with this SDK

- **Write CARDIAC programs** as listings using the six-column format from the 1968 manual (and further popularized on
  [Brian Stuart's CARDIAC page](https://www.cs.drexel.edu/~bls96/museum/cardiac.html)).

- **Assemble them into binaries**: the cards CARDIAC loads, which any CARDIAC simulator can run.

- **Run them**, from the command line or in a pipeline, with their input from the command line or standard input.

- **Debug them** with breakpoints by label, single steps, and watches that catch a program rewriting its own code.

- **Profile them**: where the instructions went, the hottest loop, the code that never ran.

- **Build on them**: everything you need is open source, in this repo.


## Want an IDE?
[Cardigan](https://lab.strangequery.com/cardigan/) is a full-on browser-based IDE that builds on cardiaq to let you develop, run and debug CARDIAC programs in your browser. Cardigan is built on a WebAssembly build of the cardiaq SDK.

## What's in this repo

| | | resembles |
|---|---|---|
| [`carc`](#carc) | the assembler: compiles six-column source code (`NAME.cq`) into decks (`NAME.cqb`) | `gcc` |
| [`burn`](#burn) | the runtime: runs compiled decks in a fast interpreter | `java`, `dotnet` |
| [`ekg`](#ekg) | the debugger and profiler | `gdb`, `perf` |
| [`pace`](#pace-lub-and-dub) | codes a channel of audio as a tape for CARDIAC to decode | `lame`, `flac` |
| [`lub`](#pace-lub-and-dub) | decodes a tape of audio on a CARDIAC and outputs the samples | `flac -d`, `mpg123` |
| [`dub`](#pace-lub-and-dub) | decodes two tapes, left and right, each on a CARDIAC, and outputs them as stereo | `flac -d`, `mpg123` |

The commands follow the GNU conventions: `--help` and `--version`, short and long options, `--` to end them,
`-` for standard input, errors as `PROG: message` or `FILE:LINE:COLUMN: error: message`, and exit status 0 for
success, 1 for failure, 2 for a mistake on the command line.

## Install

Currently no binaries are shipped. Generate them from code if you have [Rust](https://rustup.rs) installed:

    git clone https://github.com/StrangeQuery/cardiaq
    cd cardiaq
    cargo build --release

The commands are then in `target/release/`; put them on your PATH.

## Sample Walkthrough

cardiaq uses the six-column assembly language format documented on [Brian Stuart's page](https://www.cs.drexel.edu/~bls96/museum/cardiac.html):

* **Address**: the cell, 00 to 99. Cell 00 is fixed at 001, the loader uses 01 and 02 while it reads the deck in,
  and every JMP writes cell 99.
* **Contents**: a number from -999 to 999. It can be left out, and is then worked out from the opcode and operand.
* **Label**: optional, a name for the cell, mostly used for jumps and data.
* **Opcode**: the instruction's name. It is redundant: the first digit of the contents says the same thing.
* **Operand**: a cell, a label, or `label+N`.
* **Comment**: anything on the rest of the line.

A program that counts down from 5 in this format. Save it as `countdown.cq`:

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

(The filename doesn't really matter; `.cq` and `.cqb` are just conventions in this SDK. Both are text formats that any tool can create or manipulate.)


**Assemble it.** `carc` writes `countdown.cqb` beside it:

    $ carc countdown.cq
    $ cat countdown.cqb
    # countdown.cqb: countdown.cq on cards, for the loader; it starts at 10
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

The first two cards, `002` and `800`, start the bootstrap loader, and the address/contents pairs after them fill the
cells. carc generates them for you. That process ends with a JMP to the start address. In this
program that is cell 10.

**Run it.**

    $ burn countdown.cqb
    5
    4
    3
    2
    1
    0
         halted after 65 instructions

`burn` reads input from standard input and prints what the program outputs on standard output. How the program
stopped goes to standard error, so the output pipes cleanly. The count includes the 30 instructions the loader ran to
read the deck in. `ekg`, below, loads a listing through the same loader, so it runs exactly as `burn` runs its deck.

**Make a mistake.** Misspell the label in cell 13 and get cell 11's contents wrong:

    $ carc oops.cq
    oops.cq:5:6: warning: contents +131, but CLA n is +130: loading the contents
    oops.cq:7:23: error: unknown label dun
    carc: oops.cq: not assembled: no deck written

**Debug it.** Stop at `start`, look at `n`, watch it change, carry on. Commands can be typed at the `(ekg)` prompt,
or given with `-x`:

    $ ekg --batch countdown.cq -x 'break start' -x run -x 'p n' -x 'watch n' -x continue \
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
    pc 10  acc +0004  steps 36  reader []
    out: 4
    out: 3
    out: 2
    out: 1
    out: 0
    halted: HRS in cell 16, after 65 instructions

**Profile it.**

    $ ekg --profile countdown.cq 2>/dev/null
    countdown.cq: halted after 35 instructions (and 30 loading the deck), in 0.000 s (1.3 million a second)

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
| 5 | `OUT a` | output the value in cell a |
| 6 | `STO a` | store the accumulator's last three digits in cell a |
| 7 | `SUB a` | subtract cell a from the accumulator |
| 8 | `JMP a` | jump to a, first writing 8 and the address after the JMP into cell 99: the way back from a subroutine |
| 9 | `HRS a` | halt, with the program counter on a |

There is no index register: a program steps through an array by rewriting the address in one of its own
instructions. That is what `ekg`'s `watch` is for.

## carc

`carc` is the assembler: it makes a listing into a deck.

    carc countdown.cq                           # writes countdown.cqb beside it
    carc --start-at=10 -o out.cqb countdown.cq  # options before or after the file
    carc - < countdown.cq | burn -              # the listing from standard input, the deck to standard output

**Listings.** A listing has a cell a line, in Brian Stuart's six columns, `ADDRESS CONTENTS LABEL OPCODE OPERAND
COMMENT`, or the manual's `ADDRESS CONTENTS COMMENT`. The contents may be left out, and are then worked out from the
opcode and the operand: a number, a `label`, or `label+N`. `DATA` (or `DAT`) puts a number in a cell. Lines that
aren't cells (titles, prose, `;` and `#` comments) are skipped.

**Where it starts.** The deck starts the program at `--start-at`, or else where the listing says: a `Start at NN.`
line, the line labelled `start`, its first instruction.

**Errors and warnings** are reported the way a compiler reports them, and no deck is written while there are
errors. Contents that disagree with the opcode and operand are a warning: the contents are what load.

| | |
|---|---|
| `-o`, `--output=FILE` | write the deck to FILE (`-` for standard output); by default `NAME.cqb` beside `NAME.cq`, or standard output for a listing read from standard input |
| `--start-at=NN` | start the program at cell NN |
| `-h`, `--help` | display help and exit |
| `--version` | output version information and exit |

Exit status is 0 if the deck was written, 1 if the listing has errors or a file can't be read or written, 2 for a
mistake on the command line.

## burn

`burn` is the runtime, the fastest CARDIAC emulator on the planet today.

    burn countdown.cqb           # run a deck
    burn add.cqb -- 123 456      # input cards after the deck (the -- is optional)
    echo 123 456 | burn add.cqb  # or on standard input, each card read when an INP asks for it
    carc - < add.cq | burn -     # the deck itself from standard input
    burn bench                   # run diagnostics for how fast this machine runs CARDIAC

**Decks.** A deck is plain text, one card per line (lines starting with `#` are comments). There is no auto-loader,
so decks must provide their own loading logic as documented on [Brian Stuart's page](https://www.cs.drexel.edu/~bls96/museum/cardiac.html).

**Output.** Each value the program outputs is printed as a number on a line of its own. How it stopped, and after how
many instructions, goes to standard error. For a program whose output values are characters (65 for A, 10 for a
newline), pipe the output through perl:

    burn hello.cqb | perl -CO -ne 'print chr if $_ >= 0'

**Programs in several decks.** A program too big for memory comes as `NAME.cqb`, `NAME.2.cqb`, and so on. A deck
stops with the PC on the number of the deck to load next (HRS n; on 00 it is done), and `burn`, as the operator
would, puts that deck in the reader and starts the loader at 00: any deck, in any order, as often as asked.

| | |
|---|---|
| `--limit=N` | stop a program after N instructions (default a billion) |
| `-h`, `--help` | display help and exit |
| `--version` | output version information and exit |

Options go before the deck: everything after it is input, minus signs and all. Exit status is 0 if the program halted
or ran out of cards, 1 if the deck can't be read or the program never stops (a run stops at a billion instructions, or `--limit`),
2 for a mistake on the command line.

## ekg

`ekg` is the debugger and the profiler. It takes a listing (assembled as `carc` assembles it, its labels and comments
kept) or a deck (loaded by its own loader, as `burn` loads it, and disassembled), told apart by what is in it, and the
program's input after it:

    ekg countdown.cq             # debug, at the (ekg) prompt
    ekg --batch mul.cq -x 'break 17' -x run -x 'p p' -- 6 7
    ekg --profile mul.cq -- 6 7  # run it to the end and report where the instructions went

**Debugging.** The commands are gdb's where gdb has one. A PLACE is a cell (00-99), a label, or `label+N`.

| | |
|---|---|
| `run`, `r` | start the program again from the beginning, with its input, and continue |
| `continue`, `c` | run until a breakpoint, a watch, a halt, or an INP with no card |
| `step`, `s` [N] | run one instruction (or N) |
| `next`, `n` [N] | step, but run a routine a JMP calls through, on to the cell after the JMP (a JMP calls when the code it jumps to reads 99, the way back, before another JMP) |
| `break`, `b` PLACE | stop before the instruction in PLACE runs |
| `watch` PLACE | stop after an instruction writes PLACE |
| `delete`, `d` [PLACE] | remove the breakpoint and watch on PLACE (all of them without one) |
| `info break`, `i b` | the breakpoints and watches |
| `info registers`, `i r` | the PC, the accumulator, the instructions run, the cards in the reader |
| `print`, `p` PLACE | a cell's word, and the word as an instruction (also `p acc`, `p pc`) |
| `x` PLACE [N] | the words in N cells from PLACE (10 unless given) |
| `list`, `l` [PLACE] | the listing around the PC (or PLACE), the PC marked `>`, breakpoints `b` |
| `set` PLACE VALUE | write a cell (also `set acc VALUE`, `set pc PLACE`) |
| `input` CARDS... | put cards in the reader, after those in it |
| `help`, `h` | the commands |
| `quit`, `q` | leave |

An empty line repeats the last `step`, `next` or `continue`. Each stop shows the next instruction as the listing has
it, decoded afresh where the program has rewritten the cell. What the program outputs is printed as it is output,
each value on a line starting `out:`. An INP with no card left stops and waits for `input`.

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
| `--start-at=NN` | start a listing at cell NN (default: where `carc` would start it) |
| `--help` | display help and exit |
| `--version` | output version information and exit |

Exit status is 0 on success, 1 when something failed (a listing with errors; with `--batch`, a command; in a profile,
a run stopped at the limit), 2 for a usage error.

## pace, lub and dub

Audio a CARDIAC decodes. `pace` codes one channel of raw samples (signed 16-bit, little-endian) as a tape of
cards; `lub` runs the decoder, [`audio/decode.cq`](audio/decode.cq), on a CARDIAC, and outputs each sample as the
CARDIAC decodes it:

    ffmpeg -i song.wav -af 'pan=mono|c0=c0' -ar 48000 -f s16le - | pace > song.left.tape
    lub song.left.tape | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout mono -

`dub` plays stereo: two tapes, each decoded by a CARDIAC of its own. The two take turns on one thread, a block of
64 samples each, and `dub` writes their samples left then right:

    ffmpeg -i song.wav -af 'pan=mono|c0=c1' -ar 48000 -f s16le - | pace > song.right.tape
    dub song.left.tape song.right.tape | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout stereo -

The coding is a decimal ADPCM made for CARDIAC's arithmetic: blocks of 64 samples, each with a step of M x 10^K,
and a three-digit code a sample that corrects a prediction, the line through the last two samples. The decoder
keeps each sample in two cells and outputs it as two values, the thousands and then the units, because -999 to
999 is too few levels for good sound; `lub` and `dub` put each pair back together into a 16-bit sample. Every prediction,
correction and carry is worked out on the CARDIAC: about 41 instructions a sample, 2 million a second for 48 kHz. How it was
built, and why it has to be this way: [docs/AUDIO.md](docs/AUDIO.md).

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

How it gets there, so far: each cell's instruction is decoded once and kept beside its value, decoded again only
when the cell is written (CARDIAC programs rewrite their own code all the time, to index arrays), from a 4 KB table
of every word's decoding, for 100 cells; the accumulator wraps by a compare, not a division; JMP writes cell 99
already decoded; a 101st decoded cell sends the bug round to 00, so no step checks for it; tracing, for saying
where a run stopped, is a compile-time switch that costs nothing when it is off; the loop allocates nothing.
Where it goes next: fewer cycles a step, and the idioms CARDIAC code is made of (an indexed load is four
instructions of self-modifying code) done as one. No JIT: an interpreter, as fast as one can be made to go.

## As a library

All the commands are built on one Rust crate, `heartburn` for now (it holds the SDK's code as well as the engine,
until the runtime and the tools are packaged separately): the machine (`cardiac`), the assembler `carc` and Cardigan
use (`assemble`), decks (`tape`), and listings in the manual's other forms (`listing`).

```rust
use heartburn::listing::{self, Listing};
use heartburn::tape::Tape;

// the manual's Program No. 1: read two cards, output their sum
let read = listing::parse("17 034\n18 035\n19 134\n20 235\n21 636\n22 536\n23 900\n")?;
let Listing::Memory(cells) = read.listing else { return Err("a deck, not a memory listing".into()) };
let tape = Tape::one(listing::deck(17, &cells)?);
let (outputs, why, steps) = tape.run(&[2, 3], 1_000)?;   // [5], Halt, 34 instructions
```

`examples/add.rs` is this, runnable: `cargo run --example add`. `cardiac::run` runs a deck directly, and
`cardiac::Machine` is the machine a debugger holds: step it, run it to a breakpoint, read and write it. Breakpoints
cost nothing per instruction, and a plain run doesn't pay for any of it.

## License

MIT.
