"""heartburn.wasm against native heartburn: the same session scripts, run on cardiac::Machine natively
(tools/session.rs) and on heartburn.wasm in Chromium through its C API (test/session.js), must print the same: every
stop and where, the cards punched, steps, accumulator, PC, phase, memory, each cell's owner tape and write step, the
trail and the cards left in the reader.

    cargo build --release --bin session
    cargo build --release --lib --target wasm32-unknown-unknown
    python3 test/compare.py

Needs Playwright for Python and its Chromium. The cases are the manual's programs (tests/manual), sortem in two
tapes (tests/sortem), tapes fed in any order, the quine, breakpoints, blocking input and stepping.
"""
import base64
import pathlib
import subprocess
import sys
import tempfile

from playwright.sync_api import sync_playwright

HERE = pathlib.Path(__file__).resolve().parent
CRATE = HERE.parent
ROOT = CRATE.parent
WASM = CRATE / "target" / "wasm32-unknown-unknown" / "release" / "heartburn_wasm.wasm"
NATIVE = CRATE / "target" / "release" / "session"
HEARTBURN = ROOT / "target" / "release" / "heartburn"
TMP = pathlib.Path(tempfile.mkdtemp(prefix="heartburn-wasm-"))


def sh(*args):
    r = subprocess.run([str(a) for a in args], capture_output=True, text=True)
    if r.returncode:
        sys.exit(f"error: {' '.join(map(str, args))}:\n{r.stderr}")
    return r.stdout


def cards(path):
    return [int(l) for l in pathlib.Path(path).read_text().split()]


def tape(*listing, start=None, name="t"):
    out = TMP / f"{name}.tape"
    sh(HEARTBURN, "tape", *listing, *(["--start", start] if start else []), "-o", out)
    return cards(out)


def listed(*listing, start=None, name="l"):
    """A listing's cells and start, through heartburn listing (six columns)."""
    out = TMP / f"{name}.lst"
    sh(HEARTBURN, "listing", *listing, *(["--start", start] if start else []), "-o", out)
    cells, pc = [], None
    for line in out.read_text().splitlines():
        c = line.split("\t")
        if line.startswith("Start at "):
            pc = int(line.split()[2].rstrip("."))
        elif c[0].isdigit() and len(c) > 1 and c[1]:
            cells += [int(c[0]), int(c[1])]
    return cells, pc


def words(xs):
    return " ".join(map(str, xs))


M = ROOT / "tests" / "manual"
S = ROOT / "tests" / "sortem"
CASES = {}

# the manual's programs written straight into memory, their input given card by card as INP waits
for name, start, inp, extra in [
    ("add", "17", [123, 456], []), ("counting", "20", [], []), ("countdown", "20", [], []),
    ("multiply", "07", [25, 5], []), ("reverse", "15", [269], []),
    ("double-precision-add", "50", [163, 742, 258, 500], []), ("nim", "00", [517], []),
    ("improved-nim", "52", [1, 2, 3], [M / "pile-13.txt"]),
]:
    c, pc = listed(M / f"{name}.txt", *extra, start=start, name=name)
    CASES[f"manual {name}"] = f"cells {words(c)}\npc {pc}\ninput {words(inp)}\n"
# never stops: a few budgets of it
c, pc = listed(M / "counting-loop.txt", start="21", name="loop")
CASES["manual counting-loop"] = f"cells {words(c)}\npc {pc}\nbudget 997\nruns 4\n"
# a deck, card by card, to the BLANK card and then started at 10
CASES["manual bootstrap"] = f"push {words(tape(M / 'bootstrap-addition.txt', start='10', name='boot'))}\ninput 123 456\n"
# sortem in two tapes, its 35 numbers in the reader after tape 1; breakpoints in both phases; small budgets
one, two = tape(S / "sortem-1.lst", name="s1"), tape(S / "sortem-2.lst", name="s2")
nums = [(i * 37 % 101) - 50 for i in range(35)]
CASES["sortem two tapes"] = f"tape {words(one)}\ntape {words(two)}\npush {words(one + nums)}\nbudget 5000\nruns 1000\n"
CASES["sortem breakpoints"] = (f"tape {words(one)}\ntape {words(two)}\npush {words(one + nums)}\n"
                               f"break 30\nbreak 11\nbreak 99\nruns 10000\n")
CASES["sortem input as asked"] = f"tape {words(one)}\ntape {words(two)}\npush {words(one)}\ninput {words(nums)}\n"
# tapes in any order, as often as asked: 1, 2, 1, 2, ...
t1 = [2, 800, 3, 10, 4, 110, 5, 211, 6, 610, 7, 510, 8, 902, 11, 100, 2, 803]
t2 = [2, 800, 3, 10, 4, 110, 5, 211, 6, 610, 7, 510, 8, 901, 11, 200, 2, 803]
CASES["tapes 1 2 1 2"] = f"tape {words(t1)}\ntape {words(t2)}\npush {words(t1)}\ninput 1 2 3 4 5\n"
# the quine, stepped a while and then run
CASES["quine"] = f"push {words(tape(ROOT.parent / 'cardiac-code' / 'quine' / 'quine.txt', name='quine'))}\nstep 40\n"
# a breakpoint on a cell the program writes, and state set by hand
CASES["self-modifying break"] = "cells 10 130 11 612 12 900 13 900 30 531 31 42\npc 10\nacc -1234\nbreak 12\n"
CASES["negative words and the wrap"] = "push 2 800 90 191 91 42 95 190 96 699 97 591 98 191 2 895 900\nstep 3\n"


def native(script):
    path = TMP / "case.txt"
    path.write_text(script)
    return sh(NATIVE, path)


def main():
    b64 = base64.b64encode(WASM.read_bytes()).decode()
    js = (HERE / "session.js").read_text()
    failed = 0
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page()
        page.set_content(f"<script>{js}</script>")
        page.evaluate("""async (b64) => {
            const bytes = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
            window.hb = (await WebAssembly.instantiate(bytes)).instance;
        }""", b64)
        for name, script in CASES.items():
            want = native(script)
            got = page.evaluate("(s) => runSession(window.hb, s)", script)
            same = got == want
            failed += not same
            events = want.splitlines()[0][len("events: "):].split()
            print(f"{'SAME' if same else 'DIFF'}   {name}: {len(events)} stops, {want.splitlines()[2]}")
            if not same:
                for a, b in zip(want.splitlines(), got.splitlines()):
                    if a != b:
                        print(f"    native {a[:150]}\n    wasm   {b[:150]}")
        browser.close()
    print(f"{len(CASES) - failed} of {len(CASES)} the same; heartburn.wasm is {WASM.stat().st_size} bytes "
          f"({len(b64)} as base64)")
    sys.exit(1 if failed else 0)


main()
