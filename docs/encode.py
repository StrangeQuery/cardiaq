#!/usr/bin/env python3
"""Encode one channel of raw 48 kHz stereo 16-bit audio as decode.cq's stream of cards.

    encode.py IN.s16 CHANNEL START_S SECONDS CARDS [EXPECT]

CHANNEL is 0 (left) or 1 (right). CARDS gets the stream, a card a line; EXPECT, if given, the samples the decoder
should output, a line each. Each block of 64 samples takes the step M * 10^K (M 1-9, K 0-3) with the least error,
coding closed loop against the decoder's own prediction (twice the last sample less the one before), and never
past 16 bits.
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
