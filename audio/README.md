# Audio

This folder holds an audio decoder written as a CARDIAC program, [`decode.cq`](decode.cq), and a short example
track for it, [`blue-danube.track`](blue-danube.track). The decoder is an ordinary listing: you can assemble it with
`carc`, run it with `burn`, and step through it in `ekg` or Cardigan like any other program. Fed the right cards, it
works out audio samples, one at a time.

## From a wav file to your speakers

Here is the whole process for one channel, using `ffmpeg` to read and play audio:

    ffmpeg -i song.wav -af 'pan=mono|c0=c0' -ar 48000 -f s16le song.left.s16    # 1. raw samples
    xtrack song.left.s16 > song.left.track                                       # 2. encode them into a track
    lub song.left.track | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout mono -   # 3. decode and play

1. **Raw samples.** `ffmpeg` takes the left channel of `song.wav` and writes it as raw audio at 48,000 samples per
   second. A `.s16` file is nothing but samples, one after another, each a signed 16-bit number from -32,768 to
   32,767. That's too big for CARDIAC, whose cells hold only -999 to 999.

2. **Encoding.** `xtrack` turns the samples into a track: a text file of CARDIAC cards, one number per line, every
   one between -999 and 999. It doesn't split the samples into pieces. Instead it writes down how to rebuild them,
   as described in the next section. This is the only step that runs ahead of time, on an ordinary computer.

3. **Decoding and playing.** `lub` loads `decode.cq` into a CARDIAC and gives it the track's cards as its input,
   one card each time an INP instruction asks for one. The CARDIAC rebuilds each sample and outputs it as two
   values, first the thousands and then the units. `lub` joins each pair back into a 16-bit sample, and `ffplay`
   plays the samples as they arrive.

For stereo, encode the right channel too (use `c0=c1` instead of `c0=c0`), and play both tracks with `dub`, which
runs one CARDIAC for each channel:

    dub song.left.track song.right.track | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout stereo -

## What's in a track

A track is a list of instructions for rebuilding the samples. The decoder predicts each sample from the two before
it, by drawing a straight line through them: the prediction is twice the previous sample, minus the one before that.
The track only says how far to correct each prediction.

The samples are grouped into blocks of 64. Each block starts with two cards, K and M, that set the block's step
size to M x 10^K. After those come 64 cards, one for each sample, holding a code C from -500 to 499. Each sample is
its prediction plus C times the step size. A -1 where the next block's K would be marks the end of the track.

Here is the start of `blue-danube.track`, with what each card means:

    1        K = 1  ]  this block's step size is M x 10^K = 2 x 10 = 20
    2        M = 2  ]
    332      sample 0 = prediction 0       + 332 x 20  =  6,640
    -273     sample 1 = prediction 13,280  - 273 x 20  =  7,820
    -52      sample 2 = prediction 9,000   - 52 x 20   =  7,960
    ...      61 more codes, for samples 3 to 63
    0        K = 0  ]  the next block's step size is 3 x 10^0 = 3
    3        M = 3  ]
    157      sample 64
    ...      63 more codes, for samples 65 to 127
    -1       the end of the track

The prediction for sample 1 is 2 x 6,640 - 0 = 13,280, and for sample 2 it is 2 x 7,820 - 6,640 = 9,000. Because
the track holds corrections rather than samples, every card stays small even when the samples are large.

## How the CARDIAC does it

CARDIAC has no multiply instruction, and a cell can't hold a 16-bit sample, so the decoder uses two tricks:

- **Multiplying.** To compute C x M, it jumps into a row of nine `ADD` instructions at the point that leaves exactly
  M of them to run. Multiplying by 10^K is a shift by K digits, which is a single `SFT`. The block's two header
  cards rewrite those three instructions (the jump and two shifts) at the start of each block.
- **Big numbers.** It keeps each sample in two cells: the thousands, and the units from 0 to 999. It does all of
  its arithmetic in those two parts, with a carry between them, and outputs both. So 6,640 comes out as 6, then 640.

Each sample takes about 40 instructions. [docs/AUDIO.md](../docs/AUDIO.md) goes through the design in detail,
including the first three samples above, cell by cell.

## Stepping through the example

`blue-danube.track` is 128 samples of The Blue Danube, which is 2.7 milliseconds of music: too short to hear, but
just right to follow by hand. It has 133 cards: two blocks of 64 samples, each with its two header cards, and the -1
at the end.

In `ekg`, give the track's cards as input and stop at the first OUT, where the first sample is complete:

    ekg audio/decode.cq -- $(cat audio/blue-danube.track)
    (ekg) break 59
    (ekg) run
    (ekg) print h1
    (ekg) print l1

The thousands cell `h1` holds 6 and the units cell `l1` holds 640, so the first sample is 6,640. From there, `step`
shows the next sample being built one instruction at a time.

In [Cardigan](https://lab.strangequery.com/cardigan/), open `decode.cq` and paste the track's numbers into the input
cards box.

To decode the whole track at once, `burn` prints every value the CARDIAC outputs, and `lub` writes the samples as a
`.s16` file:

    carc audio/decode.cq
    burn audio/decode.cqb -- $(cat audio/blue-danube.track)
    lub audio/blue-danube.track > blue-danube.s16

## The recording

The excerpt starts 90 seconds into The Blue Danube by Johann Strauss II, played by the United States Marine Band
around 1972. Both the music and the performance are in the public domain. We took it from
[Wikimedia Commons](https://commons.wikimedia.org/wiki/File:%22An_der_sch%C3%B6nen,_blauen_Donau%22_performed_by_the_U.S._Marine_Band.flac).
