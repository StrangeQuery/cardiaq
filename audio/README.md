# Audio

cardiaq can play music, with every sample worked out by a CARDIAC. Three tools do it together:

- `xtrack` encodes one channel of audio into a **track**, a file of CARDIAC cards.
- `lub` plays one track. It runs the decoder, [`decode.cq`](decode.cq), on a CARDIAC, feeds it the track's cards,
  and writes out the samples the CARDIAC produces.
- `dub` plays two tracks, left and right, as stereo, with one CARDIAC for each channel.

The decoder is an ordinary CARDIAC program. You can assemble it with `carc`, run it with `burn`, and step through
it in `ekg` or Cardigan like any other.

## Part 1: Using xtrack, lub and dub

### 1. Start with a wav file

The tools work on raw audio: just the samples, one after another, each a signed 16-bit number, with no header.
We give these files the extension `.s16`. `ffmpeg` can make one from almost any audio file. This takes the left
channel of `song.wav` at 48,000 samples per second:

    ffmpeg -i song.wav -af 'pan=mono|c0=c0' -ar 48000 -f s16le song.left.s16

### 2. Encode it with xtrack

    xtrack song.left.s16 > song.left.track

This is the only step that happens ahead of time. `xtrack` encodes a minute of audio in about a second.

### 3. Take a peek at the track

A track is a text file with one CARDIAC card per line, and every card is between -999 and 999. Here is the start
of the example track in this folder, [`blue-danube.track`](blue-danube.track):

    $ head -5 audio/blue-danube.track
    1
    2
    332
    -273
    -52

The samples themselves aren't in there. A sample can be as big as 32,767, which doesn't fit on a card. Instead the
track says how to rebuild each sample from the ones before it. The first two cards set a step size (here 20), and
each card after that is a small correction, one per sample. Part 2 explains exactly how.

### 4. Play it in mono

    lub song.left.track | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout mono -

`lub` writes raw samples, so `ffplay` needs to be told what they are: signed 16-bit little-endian (`-f s16le`),
48,000 per second (`-ar 48000`), and one channel. The CARDIAC decodes much faster than the music plays, so the
sound starts right away. To save the result instead of playing it:

    lub song.left.track > song.cardiac.s16

### 5. Play it in stereo

Encode the right channel as well, using `c0=c1` to pick it, then play both tracks with `dub`:

    ffmpeg -i song.wav -af 'pan=mono|c0=c1' -ar 48000 -f s16le song.right.s16
    xtrack song.right.s16 > song.right.track
    dub song.left.track song.right.track | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout stereo -

### Stepping through the example

`blue-danube.track` holds 128 samples of The Blue Danube. That's only 2.7 milliseconds of music, too short to
hear, but just right for following the decoder by hand. In `ekg`, give the track's cards as input and stop at the
first OUT, where the first sample is complete:

    ekg audio/decode.cq -- $(cat audio/blue-danube.track)
    (ekg) break 59
    (ekg) run
    (ekg) print h1
    (ekg) print l1

The cell `h1` holds 6 and the cell `l1` holds 640, so the first sample is 6,640. From there, `step` shows the next
sample being built one instruction at a time. In [Cardigan](https://lab.strangequery.com/cardigan/), open
`decode.cq` and paste the track's numbers into the input cards box.

## Part 2: How these tools work

### Predict, then correct

The encoding is a kind of ADPCM (adaptive differential pulse-code modulation), the same family of codecs that
played the sound in a lot of 1990s video games. Music changes smoothly from one sample to the next, so the next
sample is usually close to what you'd guess from the last few. The decoder guesses by drawing a straight line
through the previous two samples: the prediction is twice the previous sample, minus the one before that. The
track doesn't store the samples. It stores how far off each prediction is.

### Trading multiplication for powers of ten

Every ADPCM decoder has to multiply. Each correction in the track is a small code, and the decoder multiplies it by
a step size to get the actual correction. Loud passages need big steps, and soft passages need small ones, so the
step size changes as the music does.

CARDIAC has no multiply instruction. Doing a full multiplication in a loop of ADDs would cost hundreds of
instructions per sample. But CARDIAC is a decimal machine, and it has one thing binary machines don't: `SFT`
shifts the accumulator by whole decimal digits. Multiplying by 10, 100 or 1,000 costs a single instruction.

So the step size is restricted to a single digit times a power of ten: M x 10^K, with M from 1 to 9 and K from 0
to 3. That gives 36 step sizes, from 1 up to 9,000, which is plenty. Multiplying a code by one of them is cheap:

- **x 10^K** is one `SFT`.
- **x M** is at most nine `ADD`s. The decoder has a row of nine `ADD c` instructions, and it jumps into the row at
  the point that leaves exactly M of them to run.

Binary codecs make the same trade with powers of two, because shifting is cheap for them too. CARDIAC's version is
decimal.

### Blocks, and a decoder that rewrites itself

The samples are grouped into blocks of 64, and each block has its own step size. The encoder tries all 36 step
sizes on each block and keeps the one that comes closest to the real music. A track is laid out like this:

    1        K = 1  ]  the block's step size is M x 10^K = 2 x 10 = 20
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

At the start of each block, the decoder reads K and M and writes three of its own instructions: the jump into the
row of `ADD`s, and the two `SFT`s. Rewriting your own code is how CARDIAC programs do most things, since the machine
has no index register.

### Numbers too big for a cell

A cell holds -999 to 999, and the accumulator holds -9,999 to 9,999. A 16-bit sample is bigger than either. So the
decoder keeps each sample in two cells, the thousands and the units, and does all of its arithmetic in those two
parts, with a carry between them. It outputs both parts, which is why 6,640 comes out of the CARDIAC as 6 and then
640, and why `lub` and `dub` have to join each pair back into one number.

There is one more trick. Before adding up the units, the decoder adds 2,000, so the sum can never go negative. That
way, `STO` (which keeps only the last three digits) gives the new units directly, and one `SFT` gives the carry.

### What it costs

Each sample takes about 40 instructions, about 2 million instructions per second for one channel at 48 kHz.
`burn` runs about a billion per second, so decoding is a small fraction of one core. The decoder uses 83 of
CARDIAC's 96 free cells. The sound comes out almost exactly like the original: on our test clip, the difference
is about 78 dB quieter than the music.

[docs/AUDIO.md](../docs/AUDIO.md) tells the whole story of how the design was chosen and measured, including
the first three samples worked through cell by cell.

## The recording

The example excerpt starts 90 seconds into The Blue Danube by Johann Strauss II, played by the United States Marine
Band around 1972. Both the music and the performance are in the public domain. We took it from
[Wikimedia Commons](https://commons.wikimedia.org/wiki/File:%22An_der_sch%C3%B6nen,_blauen_Donau%22_performed_by_the_U.S._Marine_Band.flac).
