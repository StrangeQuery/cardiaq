#!/usr/bin/env python3
"""Draw prediction.svg: a stretch of decoded samples, each with the decoder's prediction (the line through the two
before it) and the correction C * step that takes the prediction to the sample.

    make_prediction.py CARDS EXPECT FIRST COUNT OUT.svg

CARDS and EXPECT are encode.py's outputs for one channel; FIRST and COUNT pick the samples (inside one block).
"""
import sys

cards_path, expect_path, first, count, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), sys.argv[5]
cards = [int(x) for x in open(cards_path)]
s = [int(x) for x in open(expect_path)]
b = first // 64
k, m = cards[b * 66], cards[b * 66 + 1]
step = m * 10**k
code = lambda i: cards[b * 66 + 2 + i % 64]

idx = list(range(first, first + count))
pred = {i: 2 * s[i - 1] - s[i - 2] for i in idx}
lo = min(min(s[i] for i in idx), min(pred.values()), s[first - 2], s[first - 1])
hi = max(max(s[i] for i in idx), max(pred.values()), s[first - 2], s[first - 1])
pad = (hi - lo) * 0.08
lo, hi = lo - pad, hi + pad

W, H, L, R, T, B = 820, 400, 70, 20, 40, 50
X = lambda i: L + (i - (first - 2)) * (W - L - R) / (count + 1)
Y = lambda v: T + (hi - v) * (H - T - B) / (hi - lo)

ink, faint, pc, cc, bg = "#1f2430", "#9aa3b5", "#7a5af5", "#e0603a", "#ffffff"
o = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" font-family="Helvetica, Arial, sans-serif" font-size="13">',
     f'<rect width="{W}" height="{H}" fill="{bg}"/>',
     '<defs><marker id="a" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">'
     f'<path d="M0,0 L10,5 L0,10 z" fill="{cc}"/></marker></defs>']
# axis and zero line
for v in range(int(lo // 1000) * 1000, int(hi) + 1, 1000):
    if v < lo: continue
    o.append(f'<line x1="{L}" x2="{W - R}" y1="{Y(v):.1f}" y2="{Y(v):.1f}" stroke="#eceef3"/>')
    o.append(f'<text x="{L - 8}" y="{Y(v) + 4:.1f}" text-anchor="end" fill="{faint}">{v:,}</text>')
o.append(f'<text x="{L}" y="{T - 18}" fill="{ink}" font-weight="bold">Block {b}: step {m} x 10^{k} = {step}. Samples {first}-{first + count - 1} of the left channel</text>')
# the samples, joined
pts = " ".join(f"{X(i):.1f},{Y(s[i]):.1f}" for i in range(first - 2, first + count))
o.append(f'<polyline points="{pts}" fill="none" stroke="#c9ceda" stroke-width="1.5"/>')
# each prediction: the line through the two before, extended; then the correction
for i in idx:
    o.append(f'<line x1="{X(i - 1):.1f}" y1="{Y(s[i - 1]):.1f}" x2="{X(i):.1f}" y2="{Y(pred[i]):.1f}" stroke="{pc}" stroke-width="1.2" stroke-dasharray="4 3"/>')
    o.append(f'<circle cx="{X(i):.1f}" cy="{Y(pred[i]):.1f}" r="4.5" fill="{bg}" stroke="{pc}" stroke-width="1.8"/>')
    if abs(Y(s[i]) - Y(pred[i])) > 8:
        o.append(f'<line x1="{X(i):.1f}" y1="{Y(pred[i]):.1f}" x2="{X(i):.1f}" y2="{Y(s[i]):.1f}" stroke="{cc}" stroke-width="2" marker-end="url(#a)"/>')
for i in range(first - 2, first + count):
    o.append(f'<circle cx="{X(i):.1f}" cy="{Y(s[i]):.1f}" r="4.5" fill="{ink}"/>')
# one sample spelled out
j = idx[len(idx) // 2]
c = code(j)
o.append(f'<text x="{X(j) + 10:.1f}" y="{Y(pred[j]) + (4 if pred[j] > s[j] else 14):.1f}" fill="{pc}">prediction 2({s[j - 1]:,}) - ({s[j - 2]:,}) = {pred[j]:,}</text>')
o.append(f'<text x="{X(j) + 10:.1f}" y="{(Y(pred[j]) + Y(s[j])) / 2 + 4:.1f}" fill="{cc}">C = {c}: {c} x {step} = {c * step:,}</text>')
o.append(f'<text x="{X(j) + 10:.1f}" y="{Y(s[j]) + (24 if pred[j] > s[j] else -10):.1f}" fill="{ink}">sample {s[j]:,}</text>')
# legend
lx, ly = L, H - 18
o.append(f'<circle cx="{lx + 5}" cy="{ly - 4}" r="4.5" fill="{ink}"/><text x="{lx + 15}" y="{ly}" fill="{ink}">sample (what OUT outputs)</text>')
o.append(f'<circle cx="{lx + 215}" cy="{ly - 4}" r="4.5" fill="{bg}" stroke="{pc}" stroke-width="1.8"/><text x="{lx + 225}" y="{ly}" fill="{ink}">prediction: the line through the two before</text>')
o.append(f'<line x1="{lx + 520}" y1="{ly - 12}" x2="{lx + 520}" y2="{ly + 2}" stroke="{cc}" stroke-width="2" marker-end="url(#a)"/><text x="{lx + 530}" y="{ly}" fill="{ink}">correction: C x step, from the card</text>')
o.append("</svg>")
open(out, "w").write("\n".join(o) + "\n")
