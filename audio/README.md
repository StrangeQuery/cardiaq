# Audio

This folder holds the CARDIAC audio decoder and a short track you can step through.

- [`decode.cq`](decode.cq) is the decoder. It reads a track and outputs 16-bit audio samples, two values per
  sample: first the thousands, then the units. [docs/AUDIO.md](../docs/AUDIO.md) explains how it works.
- [`blue-danube.track`](blue-danube.track) is 128 samples (2.7 milliseconds) of The Blue Danube, encoded by
  `xtrack`. It has 133 cards: two blocks of 64 samples, each starting with its two header cards, and a -1 at the end.

## Stepping through it

The track is short enough to follow by hand. In `ekg`, give the track's cards as input and stop at the first OUT:

    ekg audio/decode.cq -- $(cat audio/blue-danube.track)
    (ekg) break 59
    (ekg) run
    (ekg) print h1
    (ekg) print l1

The first sample's thousands are 6 and its units are 640, so the sample is 6,640. `step` from there shows the next
sample being made, one instruction at a time.

In [Cardigan](https://lab.strangequery.com/cardigan/), open `decode.cq` and paste the track's numbers into the input
cards box.

To decode the whole track at once, `burn` prints every value and `lub` writes the samples:

    burn audio/decode.cqb -- $(cat audio/blue-danube.track)
    lub audio/blue-danube.track > blue-danube.s16

(Make `audio/decode.cqb` first with `carc audio/decode.cq`.)

## Where the recording comes from

The excerpt starts 90 seconds into "An der schönen, blauen Donau" (The Blue Danube), Op. 314, by Johann Strauss II,
arranged for wind ensemble and performed by the United States Marine Band, conducted by Albert F. Schoepper, around
1972. The composition is in the public domain because Strauss died in 1899. The performance is in the public domain
as a work of the US federal government. We took it from
[Wikimedia Commons](https://commons.wikimedia.org/wiki/File:%22An_der_sch%C3%B6nen,_blauen_Donau%22_performed_by_the_U.S._Marine_Band.flac),
mixed it to mono and resampled it to 48 kHz before encoding.
