# Playing audio with CARDIAC math

Our goal was to play CD-quality sound (48,000 samples per second, 16 bits per sample) in which every sample is
calculated by a CARDIAC. The host computer is allowed to feed cards to the machine and to send the numbers it
outputs to the speakers. It is not allowed to do the arithmetic.

We tested everything on a stereo soundtrack four minutes and 45 seconds long. It has music, voices, and quiet room
noise under the voices. The quiet passages turned out to be the hardest part to get right.

Part 1 builds a mono player. It covers the coding scheme, the decoder written as a CARDIAC program, the encoder,
the shell pipe that first played a channel, and finally `lub`, the mono player as a program. Part 2 adds stereo and
ends with `dub`.

Everything described here is in this repository:

- The decoder is the CARDIAC program [`audio/decode.cq`](../audio/decode.cq).
- The encoder and the decoder's assembly live in the library module `heartburn::audio`
  ([`src/audio.rs`](../src/audio.rs)).
- `xtrack` encodes a channel, `lub` plays one channel, and `dub` plays two ([`src/bin/`](../src/bin/)).
- [`encode.py`](encode.py), next to this page, is the encoder as we first wrote it, in Python. We used it as the
  reference when we wrote the Rust version.

## Part 1: A mono player

### What CARDIAC gives us

- **Memory.** 100 cells, each holding three digits and a sign (-999 to 999). Cell 00 always holds 001, the
  loader uses cells 01 and 02, and every JMP writes to cell 99. That leaves a program cells 03 to 98, which is 96
  cells.
- **The accumulator.** Four digits and a sign (-9999 to 9999).
- **Instructions.** ADD, SUB, CLA, STO, TAC (jump if negative), JMP, INP, OUT and HRS. There is no multiply and
  no divide.
- **SFT** shifts the accumulator's digits left and then right by whole decimal places. It keeps the sign.
- **STO** stores only the last three digits of the accumulator, with its sign. For example, 2,040 is stored as
  40, and -2,040 as -40.

### The overall design: tracks in, samples out

Everything we built follows the same path:

1. The host reads the audio file.
2. The host encodes each channel ahead of time into a **track**. A track is a file of CARDIAC cards, one per line.
3. A CARDIAC loads the decoder. Its INP instructions then read the track's cards one at a time, and it outputs
   two values for every sample.
4. The host combines those values into an audio stream and sends it to a player or to a file.

It has to work this way because a single CARDIAC value doesn't have enough levels for good sound. A value from
-999 to 999 has 1,999 possible levels, which is about 11 bits. CD audio has 65,536 levels (16 bits). With only one
value per sample, quiet passages like a voice over room noise would get just a handful of levels, and you would
hear hiss under every one of them. So each sample has to leave the CARDIAC as more than one value, and something
has to put the values back together in the format the sound card expects. That is the host's last step. It is a
change of format, like printing a number in decimal. All of the decoding (every prediction, every correction and
every carry) happens on the CARDIAC.

```mermaid
flowchart TD
  A["audio in"] --> H1["host: the encoder writes a track for each channel"]
  H1 --> T1["tracks: one card per code, and a header for each block"]
  T1 --> C["CARDIACs running decode.cq, one per channel"]
  C --> T2["output: two values per sample, the thousands and the units"]
  T2 --> H2["host: combines the values into 16-bit samples and interleaves the channels"]
  H2 --> O["audio out: a player, or a file"]
```

### Why not MP3 or AAC?

The soundtrack's compressed formats are built on mathematical transforms that are full of multiplications.
CARDIAC would have to do every one of those multiplications as a loop of ADDs. That would take a couple of
processor cores' worth of work, and it would be a large program to write. Small files aren't our goal anyway. We
wanted a decoder simple enough to run exactly on CARDIAC.

### ADPCM: predict, then correct

ADPCM (adaptive differential pulse-code modulation) predicts each sample from the samples before it. Instead of the
sample itself, it sends only a correction: a small code, multiplied by a step size. To decode a sample you make the
prediction and add the correction. The hard part is choosing each code, and that is the encoder's job. The encoder
runs once, ahead of time, on an ordinary computer. That division of labour suits CARDIAC well.

### How many digits per sample?

A card holds three digits. We tried both extremes:

- **One digit per sample**, so three samples fit on each card. That gives ten levels, about 3.3 bits, which is
  similar to the 4-bit ADPCM used in 90s video games.
- **Three digits per sample**, so one sample per card. The code runs from -500 to 499.

File size wasn't the goal, so the number of cards didn't decide it. On our 16-second test clip (from 107 s to 123
s, a voice over room noise), one digit per sample reached 31 dB signal-to-noise, and you could hear hiss under the
voice. Three digits reached 69.7 dB, and by ear it couldn't be told apart from the source. We use three digits.

### Adapting the step size

Our first version adjusted the step size as it went, based on the codes themselves, the way IMA ADPCM does. A large
code made the step grow and a small one made it shrink. That worked badly on this soundtrack, which is mixed loud
and heavily limited:

| Clip | 1 digit | 3 digits |
|---|---|---|
| 16 s from 107 s, step adapted from the codes | 23.6 dB | 29.0 dB |

With three digits, 50,593 of the clip's 768,000 codes hit the ends of their range. The step size couldn't grow
fast enough to keep up with the music (this is called slope overload). Rules that made the step grow faster helped
three digits (up to 49.1 dB on 8 seconds) but made one digit worse (down to 19.0 dB).

The fix is the one used by the sound chips in the PlayStation and the SNES. The encoder chooses the step size for
each short block of samples and sends it as a header at the start of the block. On CARDIAC that costs only a few
extra cards. These are the results for 4 seconds starting at 107 s:

| Block | 1 digit | 3 digits |
|---|---|---|
| 16 samples | 32.4 dB | |
| 28 samples | 31.9 dB | |
| 32 samples | | 71.1 dB |
| 64 samples | | 70.3 dB |

We use blocks of 64 samples, so there are two header cards for every 64 code cards.

### Step sizes CARDIAC can multiply by

Every sample needs the code C multiplied by the step size, and CARDIAC can't multiply. So we restricted the step
size to M x 10^K, where M is from 1 to 9 and K is from 0 to 3:

- **Multiplying by M** uses a row of nine `ADD c` instructions. The decoder jumps into the row at the point that
  leaves exactly M of them to run.
- **Multiplying by 10^K** is a shift of K digits, which is a single SFT instruction.

K stops at 3 because the shift that extracts the thousands is `SFT 0(3-K)`. Our first attempt allowed only 1, 2
or 5 for M. Allowing any value from 1 to 9 costs nothing extra, because the row of ADDs is there anyway, and it
gives the encoder finer steps to choose from.

The largest product, C x M, is 499 x 9 = 4,491. That still fits in the accumulator.

### The prediction, and the 106-cell problem

We considered letting each block choose between two predictions:

- **the previous sample**, or
- **a straight line through the previous two samples**: twice the previous sample, minus the one before it. This
  needs only additions and a subtraction.

A decoder that let each block choose came to about 106 cells, and only 96 are free. The choice would need another
header card, which would rewrite four more instructions (an ADD or SUB of the sample history, or of a zero), plus
the code to build those instructions. So we measured what the choice was actually worth, using one second at each
of eight points in the soundtrack:

| Clip (s) | Either, chosen per block | Line only | Previous sample only |
|---|---|---|---|
| 20 | 60.2 | 59.6 | 60.1 |
| 60 | 70.9 | 68.5 | 68.6 |
| 110 | 70.1 | 68.3 | 69.4 |
| 140 | 76.8 | 75.7 | 75.3 |
| 160 | 111.4 | 111.4 | 96.9 |
| 200 | 89.9 | 89.9 | 84.4 |
| 230 | 105.9 | 105.9 | 105.9 |
| 270 | 70.0 | 64.8 | 69.0 |

(signal-to-noise, in dB)

Using the straight line on its own costs at most 5 dB, and it never drops below 59.6 dB. That compromise fits in
memory. So the straight line is the only prediction, and each block's header is just two cards, K and M.

The figure below shows the prediction at work on twelve real samples. Each hollow circle is where the line through
the two previous samples lands. Each arrow is the correction, C times the block's step size, that moves the
prediction to the actual sample. This block is noisy, so the corrections are large. On smooth music the circles
sit almost on top of the samples, and the codes are small.

![The decoder's prediction and correction on samples 8-19 of the test clip's left channel](figures/prediction.svg)

### Fitting a 16-bit sample into three-digit cells

A 16-bit sample runs from -32,768 to 32,767. That fits neither in a cell nor in the accumulator. So the decoder
keeps each sample in two cells: H, the thousands, and L, the units from 0 to 999. The sample is H x 1000 + L. For
example, -12,345 is stored as H = -13 and L = 655.

The correction C x M x 10^K is split the same way, into a thousands part and a units part, by two SFT
instructions. Both parts keep the correction's sign. To keep the sum of the units from going negative, the decoder
adds a bias of 2000:

    sum = 2000 + 2 L1 - L2 + units       (between 2 and 4,997)
    L   = the last three digits of sum    (STO keeps exactly these)
    H   = (sum shifted right three digits, the carry from 0 to 4) + 2 H1 - H2 + thousands - 2

Here L1 and H1 are the previous sample, and L2 and H2 are the one before it. The bias means the carry comes out of
a plain shift, and the -2 at the end removes the bias again.

### The block header rewrites the program

Each block's header is two numbers, K and M. The decoder uses them to write three of its own instructions:

| Cell | Becomes | Built as |
|---|---|---|
| `mul` | a JMP into the row of ADDs, M instructions from its end | `JMP x1`, minus M, plus 1 |
| `shi` | SFT 0(3-K), which extracts the thousands | 403 - K |
| `slo` | SFT (K+1)1, which extracts the units | 411 + 10 K |

That way the track contains only plain numbers, not instructions that depend on where the decoder keeps things in
memory. A K below zero marks the end of the track.

### A bug: a product too big for a cell

The first version of the decoder stored C x M in a cell and then shifted the thousands and the units out of that
cell. But a cell holds only three digits, and C x M can have four, so the thousands were lost. The very first
sample of the test came out as 40 instead of 2,040. Comparing against the encoder's expected output caught it on
sample 0, and stepping through in `ekg` showed why: after `SFT 03`, the thousands were 0, because the cell being
shifted held 040.

The fix costs nothing. The units depend only on the last three digits, which is exactly what STO keeps. So the
decoder stores C x M (keeping its last three digits), shifts the thousands out of the accumulator, which still holds
all four digits, and then reloads the stored digits to get the units.

### What the decoder does for each sample

| Cells | Instructions | What happens |
|---|---|---|
| 19-21 | 3 | INP reads the code C, the accumulator is cleared, and the program jumps into the row of ADDs |
| 22-30 | M | the row of ADDs computes C x M |
| 31-36 | 6 | multiply by 10^K and split into thousands and units |
| 37-43 | 7 | the units: 2000 + 2 L1 - L2 + units; STO keeps the new L |
| 44-50 | 7 | the thousands: carry + 2 H1 - H2 + thousands - 2 |
| 51-58 | 8 | the previous sample becomes the one before it, and the new sample becomes the previous one |
| 59-60 | 2 | OUT the thousands, then OUT the units |
| 61-65 | 5 | count down the block; at the end of the block, go back and read the next header |

That is 38 + M instructions per sample, plus 16 instructions for each block's header.

```mermaid
flowchart TD
  K["INP K"] --> E{"K < 0?"}
  E -->|yes| X["HRS: the end of the track"]
  E -->|no| W["write SFT 0(3-K) and SFT (K+1)1;<br/>INP M; write the JMP into the row of ADDs"]
  W --> N["count = 63"]
  N --> C["INP C"]
  C --> M["C x M: jump into the row and run M ADDs"]
  M --> S["multiply by 10^K and split: STO keeps the last three digits;<br/>SFT 0(3-K) gives the thousands, SFT (K+1)1 the units"]
  S --> U["units: 2000 + 2 L1 - L2 + units;<br/>STO keeps the new L, SFT 03 gives the carry"]
  U --> T["thousands: carry + 2 H1 - H2 + thousands - 2"]
  T --> R["move the sample history along"]
  R --> O["OUT H, OUT L"]
  O --> Q{"count - 1 < 0?"}
  Q -->|no| C
  Q -->|yes| K
```

### The first three samples, cell by cell

The left channel of the test clip starts with the cards `0 6 332 -458 143`. That is a header with K = 0 and M = 6
(a step size of 6), followed by the first three codes. The decoder starts with every history cell at 0.

| | Sample 0 | Sample 1 | Sample 2 |
|---|---|---|---|
| The card C | 332 | -458 | 143 |
| C x M | 1,992 | -2,748 | 858 |
| Thousands (SFT 03) | 1 | -2 | 0 |
| Units (STO keeps the last three digits; then SFT 11) | 992 | -748 | 858 |
| History before: H1, L1 and H2, L2 | 0, 0 and 0, 0 | 1, 992 and 0, 0 | 1, 236 and 1, 992 |
| Units sum: 2000 + 2 L1 - L2 + units | 2,992 | 3,236 | 2,338 |
| New L, and the carry | 992, carry 2 | 236, carry 3 | 338, carry 2 |
| New H: carry + 2 H1 - H2 + thousands - 2 | 2 + 0 - 0 + 1 - 2 = 1 | 3 + 2 - 0 - 2 - 2 = 1 | 2 + 2 - 1 + 0 - 2 = 1 |
| OUT | 1, then 992 | 1, then 236 | 1, then 338 |
| The sample | 1,992 | 1,236 | 1,338 |
| Check: prediction + C x step | 0 + 1,992 | 3,984 - 2,748 | 480 + 858 |

The prediction for sample 1 is 2 x 1,992 - 0 = 3,984, and the prediction for sample 2 is 2 x 1,236 - 1,992 = 480.
The decoder never forms those five-digit numbers. It works in thousands and units the whole time, and the carry
joins the two.

### The decoder listing

This is [`audio/decode.cq`](../audio/decode.cq), in cardiaq's six-column listing format. `carc audio/decode.cq`
turns it into the deck `audio/decode.cqb`. `lub` and `dub` have the listing built in and assemble it the same way
`carc` does.

```
; Decimal ADPCM decoder. It reads a track of cards and outputs 16-bit samples. Each sample is output as two
; values: first the thousands, then the units (0-999). The sample is thousands * 1000 + units.
;
; A track is made of blocks of 64 samples. Each block starts with two header cards, K (0-3) and M (1-9), and the
; block's step size is M * 10^K. After the header comes one card per sample, a code C from -500 to 499. Each sample
; is a prediction plus C times the step size. The prediction is a straight line through the previous two samples:
; twice the previous sample, minus the one before it. A K below zero marks the end of the track.
;
; Each sample is kept in two cells: H holds the thousands and L holds the units (0-999). The header writes three
; instructions into the program. The first is a jump into a row of nine ADDs, placed so that exactly M of them run;
; that multiplies C by M. The other two are SFTs that split C * M * 10^K into thousands and units: SFT 0(3-K) and
; SFT (K+1)1. Both keep the sign. C * M can have four digits, which a cell can't hold, so the thousands are shifted
; out of the accumulator, and the units come from the last three digits, which is what STO keeps. The units are
; summed on top of 2000 so the sum never goes negative. STO then keeps the new units, and SFT 03 gives the carry.
;
; Address Contents Label Opcode Operand Comment
  03            block   INP   k        03-18: the header
  04                    CLA   k
  05                    TAC   done
  06                    SFT   10       SFT (K+1)1 for the units
  07                    ADD   s411
  08                    STO   slo
  09                    CLA   s403     SFT 0(3-K) for the thousands
  10                    SUB   k
  11                    STO   shi
  12                    INP   m
  13                    CLA   jx1      JMP into the row, M ADDs from its end
  14                    SUB   m
  15                    ADD   00
  16                    STO   mul
  17                    CLA   n
  18                    STO   cnt
  19            sample  INP   c        19-31: C * M
  20                    CLA   z
  21            mul     DATA  0        (JMP x1+1-M)
  22                    ADD   c
  23                    ADD   c
  24                    ADD   c
  25                    ADD   c
  26                    ADD   c
  27                    ADD   c
  28                    ADD   c
  29                    ADD   c
  30            x1      ADD   c
  31                    STO   u        31-36: * 10^K, split: the units come from C * M's last three digits
  32            shi     DATA  0        (SFT 0(3-K))
  33                    STO   hi
  34                    CLA   u
  35            slo     DATA  0        (SFT (K+1)1)
  36                    STO   lo
  37                    CLA   two      37-43: the units, 2000 + 2 L1 - L2 + lo
  38                    SFT   30
  39                    ADD   l1
  40                    ADD   l1
  41                    SUB   l2
  42                    ADD   lo
  43                    STO   ln
  44                    SFT   03       44-50: the thousands, carry + 2 H1 - H2 + hi - 2
  45                    ADD   h1
  46                    ADD   h1
  47                    SUB   h2
  48                    ADD   hi
  49                    SUB   two
  50                    STO   hn
  51                    CLA   l1       51-58: the last becomes the one before
  52                    STO   l2
  53                    CLA   ln
  54                    STO   l1
  55                    CLA   h1
  56                    STO   h2
  57                    CLA   hn
  58                    STO   h1
  59                    OUT   h1       59-60: the sample
  60                    OUT   l1
  61                    CLA   cnt      61-65: the next, or the next block's header
  62                    SUB   00
  63                    STO   cnt
  64                    TAC   block
  65                    JMP   sample
  66            done    HRS   00
  70            k       DATA  0
  71            m       DATA  0
  72            c       DATA  0
  73            u       DATA  0
  74            lo      DATA  0
  75            hi      DATA  0
  76            ln      DATA  0
  77            hn      DATA  0
  78            cnt     DATA  0
  79            n       DATA  63       a block is 64 samples
  80            two     DATA  2
  81            s411    DATA  411
  82            s403    DATA  403
  83            jx1     JMP   x1
  84            z       DATA  0
  85            l1      DATA  0
  86            l2      DATA  0
  87            h1      DATA  0
  88            h2      DATA  0
```

It uses 83 of the 96 free cells: the code is in cells 03 to 66 and the data in cells 70 to 88.

### The encoder

`encode.py` encodes one channel. For each block, it tries all 36 step sizes and keeps the one with the smallest
squared error. It encodes "closed loop": it chooses each code against the sample the decoder will actually
produce, not against the original, so small errors never add up over time. It never lets a sample go outside 16
bits. Besides the cards, it can also write out the samples the decoder should produce, and that is what we check
the decoder against.

```python
#!/usr/bin/env python3
"""Encode one channel of raw 48 kHz stereo 16-bit audio as a track for decode.cq.

    encode.py IN.s16 CHANNEL START_S SECONDS CARDS [EXPECT]

CHANNEL is 0 for the left channel or 1 for the right. The track is written to CARDS, one card per line. If EXPECT
is given, the samples the decoder should output are written there, one per line. For each block of 64 samples, the
encoder picks the step size M * 10^K (M from 1 to 9, K from 0 to 3) with the smallest error. It chooses each code
against the decoder's own prediction (twice the previous sample, minus the one before it), and it never lets a
sample go outside 16 bits.
"""
import struct, sys

N = 64
STEPS = [(k, m) for k in range(4) for m in range(1, 10)]

def block(xs, s1, s2, step):
    err, out = 0, []
    for x in xs:
        e = 2 * s1 - s2
        lo, hi = -((32768 + e) // step), (32767 - e) // step
        c = max(-500, lo, min(499, hi, round((x - e) / step)))
        s = e + c * step
        err += (x - s) ** 2
        out.append((c, s))
        s2, s1 = s1, s
    return err, out

def encode(xs):
    cards, samples, s1, s2 = [], [], 0, 0
    for b in range(0, len(xs), N):
        blk = xs[b:b + N]
        blk += [blk[-1]] * (N - len(blk))
        best = min(((block(blk, s1, s2, m * 10**k), (k, m)) for k, m in STEPS), key=lambda r: r[0][0])
        (_, out), (k, m) = best
        cards += [k, m] + [c for c, _ in out]
        samples += [s for _, s in out]
        s1, s2 = out[-1][1], out[-2][1]
    return cards + [-1], samples

def main():
    src, ch, start, secs, dst = sys.argv[1], int(sys.argv[2]), float(sys.argv[3]), float(sys.argv[4]), sys.argv[5]
    with open(src, "rb") as f:
        f.seek(int(start * 48000) * 4)
        raw = f.read(int(secs * 48000) * 4)
    xs = list(struct.unpack(f"<{len(raw) // 2}h", raw)[ch::2])
    cards, samples = encode(xs)
    with open(dst, "w") as f:
        f.write("\n".join(map(str, cards)) + "\n")
    if len(sys.argv) > 6:
        with open(sys.argv[6], "w") as f:
            f.write("\n".join(map(str, samples)) + "\n")

main()
```

It is slow. It takes about 30 seconds to encode 16 seconds of one channel, and nearly all of that time goes into
trying every step size. `xtrack` is the same encoder written in Rust (`heartburn::audio::encode`). It writes exactly
the same track, card for card, in about a quarter of a second. It even rounds halves to the nearest even number,
as Python's `round` does, so that the two agree. It adds one safeguard. Sometimes the prediction overshoots so far
that a small step size can't bring the sample back inside 16 bits. When that happens, `xtrack` doesn't consider
that step size for the block. (The Python version would write a sample the decoder can't hold.) The largest step
size, 9,000, can always reach, so every block still gets a step size.

### Tracks are CARDIAC input

A track contains nothing but CARDIAC cards: one number per line, each from -999 to 999. First come each block's
two header cards, then its 64 code cards, and at the very end a -1 that makes the decoder halt. Once the decoder is
loaded, every card goes into the card reader exactly as it appears in the file, and the decoder's INP
instructions read them one by one. The host doesn't change a single card on the way in.

That is why plain `burn` can run a track with no help at all:

```sh
carc audio/decode.cq                                          # assemble the decoder into audio/decode.cqb
burn --limit=100000000000 audio/decode.cqb < song.left.track
```

`burn` loads the decoder deck and then feeds it the track as input cards, exactly as it would feed
`burn add.cqb -- 123 456`. A track is simply a very long input deck. (The `--limit` is there because `burn`
normally stops a program after a billion instructions, and a full-length channel takes more than that.)

`burn` doesn't play audio, though. It prints every value the program outputs as a decimal number on its own line.

### The first player: a shell pipe

This is how we first played a channel, using the Python encoder, `burn`, and a few lines of Python to join the
values. The input is raw 48 kHz stereo 16-bit samples, which `ffmpeg` can make from any audio file. Run these
from the top of the repository after building the tools:

```sh
B=target/release
ffmpeg -i song.wav -ar 48000 -ac 2 -f s16le song.s16
S=song.s16

# assemble the decoder once; this writes audio/decode.cqb
$B/carc audio/decode.cq

# encode the left channel, 16 seconds starting at 107 s:
# the cards, and the samples the decoder should produce
python3 docs/encode.py $S 0 107 16 left.cards left.expect

# decode it on a CARDIAC; 16 seconds of one channel is about 32 million instructions
$B/burn --limit=100000000000 audio/decode.cqb < left.cards > left.out
```

**Checking the result.** `burn` prints each output value on its own line, so every sample takes two lines: the
thousands, then the units. This joins them and compares them with the encoder's expected samples:

```sh
python3 -c '
import sys; f = sys.argv[1]
o = [int(x) for x in open(f + ".out")]
got = [o[i] * 1000 + o[i + 1] for i in range(0, len(o), 2)]
print(f, "matches" if got == [int(x) for x in open(f + ".expect")] else "DIFFERS")
' left
```

**Playing it while it decodes.** `burn` reads each card at the moment an INP asks for it, and it prints each value
at the moment OUT produces it. So a pipe can play the channel while the CARDIAC is still decoding it. A few lines
of Python join each pair of values into a 16-bit sample, and `ffplay` sends the raw samples to the speakers:

```mermaid
flowchart TD
  A["song.s16<br/>48 kHz, 16-bit"] --> E["encode.py<br/>run once, ahead of time"]
  E --> F["the track: left.cards<br/>K, M, then 64 codes, one block after another"]
  subgraph host1 ["host: burn"]
    R["the card reader"]
  end
  subgraph cardiac ["CARDIAC: decode.cq"]
    D["prediction + C x step,<br/>in thousands and units"]
  end
  subgraph host2 ["host: burn, then Python"]
    P["burn prints each value<br/>on its own line, as text"] --> J["Python joins each pair: thousands x 1000 + units,<br/>as a binary 16-bit sample"]
  end
  F --> R
  R -- "INP: one card, when asked" --> D
  D -- "OUT: the thousands, then the units" --> P
  J --> FF["ffplay: raw signed 16-bit, 48 kHz, mono"] --> SP(("speakers"))
```

```sh
$B/burn --limit=100000000000 audio/decode.cqb < left.cards \
  | python3 -c '
import sys, struct
out = sys.stdout.buffer
for h in sys.stdin:
    out.write(struct.pack("<h", int(h) * 1000 + int(next(sys.stdin))))' \
  | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout mono -
```

The Python step is the host's combining step, the last one on the path. It reads `burn`'s output two lines at a
time (a sample's thousands, then its units) and writes the sample as two binary bytes, least significant first.
For example, 1,992 goes to `ffplay` as the bytes `C8 07`. Raw samples have no header, so `ffplay` has to be told
what they are: `-f s16le` means signed 16-bit little-endian, `-ar 48000` is the sample rate, and `-ch_layout mono`
means one channel.

The CARDIAC decodes far faster than real time, so the sound card sets the speed. When the pipe is full, `burn`
waits for the speakers to catch up. We played the 16-second test clip this way, and it sounded the same as the
source.

### lub: the mono player in Rust

The pipe above has a weak spot: the joining is done by a Python one-liner. `lub` does the same job as `burn` and
the Python together, written in Rust, in about 130 lines including its help text and error messages. It runs one
CARDIAC on the decoder and feeds it a card from the track each time an INP asks for one. It joins each pair of
output values into a sample and writes the sample to standard output as soon as it is ready.

Think of the difference between a terminal's numeric mode and its character mode. OUT only ever produces numbers;
the device at the other end decides what they mean. `burn` prints them as digits. A terminal in character mode
shows 65 as "A". `lub` treats each pair of numbers as one audio sample. Every value the CARDIAC outputs arrives at
`lub`'s `output` function:

```rust
impl<W: Write> Console for Samples<W> {
    fn output(&mut self, value: i32) {
        let Some(thousands) = self.thousands.take() else {
            self.thousands = Some(value);
            return;
        };
        let Some(sample) = audio::sample(thousands, value) else {
            die(format!("the decoder output {thousands} and {value}, which aren't a 16-bit sample"))
        };
        let r = self.out.write_all(&sample.to_le_bytes());
        self.written(r);
    }
}
```

The first value of each pair is held as the thousands, and the second completes the sample. `audio::sample` checks
that the pair really is a sample (the units must be 0 to 999 and the result must fit in 16 bits) and computes
thousands x 1000 + units. The sample then goes out as two little-endian bytes.

`xtrack` makes the track, and `lub` plays it:

```sh
ffmpeg -i song.wav -af 'pan=mono|c0=c0' -ar 48000 -f s16le - | $B/xtrack > song.left.track
$B/lub song.left.track | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout mono -
```

`lub` produces exactly the same bytes as the `burn` and Python pipe. When something is wrong, it stops with a
message instead of playing noise: if a track has no end card, if a line isn't a card, or if the decoder's two
values aren't a 16-bit sample. If the player stops reading, `lub` quietly stops too. The tests
(`tests/audio.rs`, and the tests in `src/audio.rs`) check that the CARDIAC's samples exactly match the encoder's
expected samples, both through the library and through the programs.

### Results

| | |
|---|---|
| Exactness | The decoder's output matches the encoder's expected samples on all 768,000 samples of each channel of the test clip: through files, through the pipe, and through `lub`. |
| Quality | 69.2 dB signal-to-noise over the 16-second stereo clip. By ear, no different from the source. |
| Speed | 38 + M instructions per sample, 41.3 on average including the headers (31.7 million for 768,000 samples). That is about 2 million instructions per second for one channel at 48 kHz. |
| Memory | 83 of the 96 free cells. |
| Encoding | `xtrack` takes 0.25 s for 16 seconds of one channel, about 5 s for a whole channel of the soundtrack. The Python version takes 30 s for 16 seconds. |
| Track size | 66 cards for every 64 samples, plus one card at the end: 792,001 cards (3.0 MB as text) for 16 seconds of one channel. |
| The whole soundtrack | About 28 million cards for both channels. Stored as 2 bytes per card, that is 56 MB, the same size as the raw 16-bit audio. |

The listening test turned up a high whine at 15.7 kHz in the three-digit clip. It turned out to be in the source
too, at the same level. A recording of a fridge in the mix had picked up the whine of an old CRT television. The
codec reproduced it faithfully. The codec's own error over the clip is only one or two steps of a 16-bit sample.

## Part 2: Stereo

### Two channels, two CARDIACs

A stereo recording has two channels, left and right. The two channels can be decoded completely separately. Each
decoder keeps its own history of recent samples and reads its own block headers, and neither needs anything from
the other. So for stereo we run two CARDIACs, both running `decode.cq`, one for each channel, each reading its own
track.

### Stereo the batch way

The first stereo clip was made by running `burn` once for each channel and writing the two outputs into a wav file:

```sh
python3 docs/encode.py $S 0 107 16 left.cards  left.expect
python3 docs/encode.py $S 1 107 16 right.cards right.expect
$B/burn --limit=100000000000 audio/decode.cqb < left.cards  > left.out
$B/burn --limit=100000000000 audio/decode.cqb < right.cards > right.out

python3 -c '
import struct, wave
ch = []
for f in ("left", "right"):
    o = [int(x) for x in open(f + ".out")]
    ch.append([o[i] * 1000 + o[i + 1] for i in range(0, len(o), 2)])
w = wave.open("cardiac.wav", "wb"); w.setnchannels(2); w.setsampwidth(2); w.setframerate(48000)
w.writeframes(b"".join(struct.pack("<hh", a, b) for a, b in zip(*ch))); w.close()
'
afplay cardiac.wav
```

That decodes everything first and plays it afterwards. To stream stereo, the two channels' samples have to be
interleaved as they are made (left, right, left, right), and a shell pipe can't easily do that.

### dub: two CARDIACs on one thread

Decoding both channels costs about 4 million instructions per second, so giving each channel its own thread would
gain nothing and would add the work of keeping the threads in step. Instead, `dub` uses one thread and two
CARDIACs, and the two take turns, one block at a time. On each turn, a channel's CARDIAC is given the next 66 cards
of its track (one header and 64 codes). It runs until it asks for more cards, which is exactly one block of 64
samples. Then it is the other channel's turn. Each machine is still a plain mono decoder, with its own 100 cells
and its own track.

```sh
ffmpeg -i song.wav -af 'pan=mono|c0=c1' -ar 48000 -f s16le - | $B/xtrack > song.right.track
$B/dub song.left.track song.right.track | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout stereo -
```

This is the same path as the mono player, with two of everything in the middle: two tracks, two CARDIACs, and two
streams of output values. The host's combining step joins each channel's thousands and units as before. Then it
interleaves the channels, left then right, because that is what a stereo stream is. The `-ch_layout stereo` flag
tells `ffplay` to expect exactly that.

```mermaid
flowchart TD
  A["song.wav"] --> PL["ffmpeg, then xtrack: the left channel's track"]
  A --> PR["ffmpeg, then xtrack: the right channel's track"]
  subgraph dub ["dub: one thread, taking turns one block at a time"]
    CL["CARDIAC: decode.cq"]
    CR["CARDIAC: decode.cq"]
    J["joins each pair into a sample;<br/>left, right, left, right"]
  end
  PL --> CL
  PR --> CR
  CL -- "thousands, units" --> J
  CR -- "thousands, units" --> J
  J --> FF["ffplay: raw signed 16-bit, 48 kHz, stereo"] --> SP(("speakers"))
```

`dub`'s output is byte for byte identical to the stereo clip made the batch way above, and it takes 0.26 s for
the 16 seconds. If the two tracks are different lengths, `dub` reports an error that names the longer track and
says by how many samples. It doesn't play one channel on its own.

### Alternatives we rejected

- **One CARDIAC alternating between the channels.** This would need twice as many history cells, and one program
  would have to keep track of two decoders' state. It costs the same and is harder to read, so it gains nothing.
- **Mid and side.** One machine would encode left + right and the other left - right. When the two channels are
  similar, the difference needs fewer digits. But file size isn't our goal, and undoing the mid/side split would
  put arithmetic back on the host.

### Why the tracks are made ahead of time

The encoder works one block at a time, and each block depends only on its own 64 samples and the last two samples
the decoder produced. So the host could encode each block a few milliseconds before the CARDIAC needs it, and
`xtrack` is fast enough to do that on about 3% of one core for stereo.

We don't, for two reasons. First, encoding is the expensive half of the work: for every block it tries all 36
step sizes, predicting, dividing, rounding and adding up squared errors. If the host did that live, it would be
doing far more arithmetic than the CARDIACs. Second, it would be circular. To encode live, the host would need the
raw audio in memory, and then it could simply play that audio directly.

A track isn't a pre-rendered result. It is the music stored in the form the player reads, just as a demo ships a
packed music file and decodes it as it plays. Nothing is computed ahead of time except the encoding. Every sample
is still decoded by a CARDIAC at the moment it is played.

### Still to do

- A binary card format for long tracks (2 bytes per card), so that a whole soundtrack isn't 130 MB of text.
- Playing in step with a picture, which means handing the samples to the sound card on a clock, instead of as fast
  as the pipe will take them.
