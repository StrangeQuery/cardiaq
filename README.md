# HEARTBURN

CARDIAC, the 1968 Bell Labs cardboard computer, stupid fast.

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

On real programs, the 109 machines of the demo HOLOCENE (4.5 billion instructions, every card checked against the
same programs compiled natively), it runs about 760 million a second: 8 times slower than native Rust doing the
same work, with each CARDIAC instruction doing a fraction of what one native instruction does. (It started at 510
million, 12 times; the first round of tuning is in the history.)

How it gets there, so far: each cell's instruction is decoded once and kept beside its value, decoded again only
when the cell is written (CARDIAC programs rewrite their own code all the time, to index arrays), from a 4 KB table
of every word's decoding, for 100 cells; the accumulator wraps by a compare, not a division; JMP writes cell 99
already decoded; a 101st decoded cell sends the bug round to 00, so no step checks for it; tracing, for saying
where a run stopped, is a compile-time switch that costs nothing when it is off; the loop allocates nothing.
Where it goes next: fewer cycles a step, and the idioms CARDIAC code is made of (an indexed load is four
instructions of self-modifying code) done as one. No JIT: an interpreter, as fast as one can be made to go.

## Use

    heartburn run hello.tape            # each card punched, as it is punched
    heartburn run gcd.tape -- 252 105   # input after --, else standard input, read as INP asks
    heartburn asm prog.asm              # a listing to a tape: prog.tape
    heartburn cardiasm prog.cardiasm    # Jason Whittington's CARDIASM to a deck: prog.cardimg
    heartburn bench

A `.tape` is one card per line: the two-card loader, an address and a value for each cell, a card that jumps to
the start. A program too big for memory comes in phases, a tape each. By the overlay convention, a phase stops
with the bug on the number of the tape to load next (HRS n; on 00 it is done), and the operator, here HEARTBURN,
puts that tape in the reader and starts the loader at 00.

As a library, HEARTBURN is what [rust-cardiac](https://github.com/StrangeQuery/rust-cardiac), which compiles Rust
for CARDIAC, runs its programs on.
