//! `ekg`: a listing or a deck debugged by commands given with -x or piped in (breakpoints by label, watches on code
//! the program rewrites, stepping, an INP waiting for a card), and profiled (each cell's runs, the cells written,
//! the instructions never run, the program's cards apart from the report).
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn dir(test: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("heartburn-ekg-{test}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `EXE ARGS`, `stdin` on its standard input: (it succeeded, what it printed, what it said on standard error).
fn run(exe: &str, args: &[&str], stdin: &str) -> (bool, String, String) {
    let mut child = Command::new(exe)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn ekg(args: &[&str]) -> (bool, String, String) {
    run(env!("CARGO_BIN_EXE_ekg"), args, "")
}

/// `ekg --batch FILE -x C1 -x C2 ... -- INPUT`
fn script(file: &str, commands: &[&str], input: &[&str]) -> (bool, String, String) {
    let mut args = vec!["--batch", file];
    for c in commands {
        args.extend(["-x", c]);
    }
    args.push("--");
    args.extend(input);
    ekg(&args)
}

const SORTEM: &str = "tests/sortem/sortem.cq";

#[test]
fn a_breakpoint_by_label_stops_before_its_instruction() {
    let (ok, out, err) = script(
        SORTEM,
        &["break swap", "run", "info registers", "p a", "p b", "p acc"],
        &["5", "3"],
    );
    assert!(ok && err.is_empty(), "{out}{err}");
    assert!(
        out.starts_with(
            "breakpoint at 42 (swap)\nbreakpoint, cell 42\n42   189  swap     CLA  opsto    ; build STO tab+j and STO tab+j+1\n"
        ),
        "{out}"
    );
    // b - a went negative: 3 - 5
    assert!(out.contains("pc 42  acc -0002  steps "), "{out}");
    assert!(out.contains("91 (a) = 005\n"), "{out}");
    assert!(out.contains("92 (b) = 003\n"), "{out}");
    assert!(out.contains("acc = -0002\n"), "{out}");
    // a cell the listing calls code is decoded too
    let (_, out, _) = script(SORTEM, &["p swap"], &[]);
    assert!(out.contains("42 (swap) = 189    CLA 89\n"), "{out}");
    // continuing from the breakpoint runs its instruction, and the rest, to the halt
    let (ok, out, _) = script(SORTEM, &["b swap", "r", "c"], &["5", "3"]);
    assert!(ok, "{out}");
    let cards: Vec<&str> = out.lines().filter_map(|l| l.strip_prefix("out: ")).collect();
    assert_eq!(cards, ["3", "5", "4", "3", "5", "5"], "{out}");
    assert!(
        out.ends_with("halted: HRS in cell 82, after 128 instructions\n"),
        "{out}"
    );
}

/// `run` stops on a breakpoint at the program's first instruction, before it runs (as gdb does); `continue` from a
/// breakpoint runs its instruction.
#[test]
fn run_stops_on_a_breakpoint_at_the_start() {
    let f = dir("start").join("count.cq");
    std::fs::write(&f, "10 530 start OUT n\n11 130 CLA n\n12 700 SUB 00\n13 316 TAC done\n14 630 STO n\n15 810 JMP start\n16 900 done HRS 00\n30 002 n DATA 2\n").unwrap();
    let (ok, out, err) = script(f.to_str().unwrap(), &["break start", "run", "p n", "c"], &[]);
    assert!(ok && err.is_empty(), "{out}{err}");
    assert!(
        out.starts_with("breakpoint at 10 (start)\nbreakpoint, cell 10\n10   530  start    OUT  n\n30 (n) = 002\nout: 2\nbreakpoint, cell 10\n"),
        "{out}"
    );
}

#[test]
fn a_watch_stops_when_the_program_rewrites_its_code() {
    // sortem builds INP tab+i in rdi, a cell listed as DATA 0, and runs it
    let (ok, out, _) = script(
        SORTEM,
        &["watch rdi", "run", "c", "info break", "delete rdi", "c"],
        &["5", "3"],
    );
    assert!(ok, "{out}");
    assert!(
        out.contains(
            "watch, cell 08 (rdi): 000 -> 097, written by cell 07\n08   097  rdi      INP  tab      ; INP tab+i (listed 000)\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("watch, cell 08 (rdi): 097 -> 098, written by cell 07\n"),
        "{out}"
    );
    assert!(
        out.contains("watch      08   098  rdi      DATA 98       ; INP tab+i\n"),
        "{out}"
    );
    assert!(
        out.ends_with("halted: HRS in cell 82, after 128 instructions\n"),
        "{out}"
    );
}

#[test]
fn piped_commands_step_list_and_repeat() {
    let (ok, out, err) = run(
        env!("CARGO_BIN_EXE_ekg"),
        &[SORTEM, "--", "5", "3"],
        "step 3\n\nlist\nx 83 5\nset k0 7\np k0\nfrobnicate\nq\nstep\n",
    );
    // an empty line repeats the step 3
    assert!(ok, "{out}");
    assert!(
        out.starts_with("06   287           ADD  i\n09   187           CLA  i\n"),
        "{out}"
    );
    assert!(out.contains("\n > 09   187           CLA  i\n"), "{out}");
    assert!(
        out.contains("\n   08   097  rdi      DATA 97       ; INP tab+i\n"),
        "{out}"
    );
    assert!(out.contains("\n83:  001  000  002  197  000\n"), "{out}");
    // nothing after quit; a command that isn't one said on standard error
    assert!(out.ends_with("84 (k0) = 007\n"), "{out}");
    assert_eq!(err, "frobnicate: not a command (help lists them)\n");
}

#[test]
fn next_runs_a_routine_through() {
    // 10 JMP twice, a routine that doubles n and returns the CARDIAC way: it keeps the JMP the call left in 99 as
    // its last instruction; 11 OUT n, 12 HRS
    let f = dir("next").join("call.cq");
    std::fs::write(
        &f,
        "10 start JMP twice\n11 OUT n\n12 HRS\n20 twice CLA 99\n21 STO ret\n22 CLA n\n23 ADD n\n24 STO n\n\
         25 ret DATA 0\n30 n DATA 21\n",
    )
    .unwrap();
    let f = f.to_str().unwrap();
    let (ok, out, _) = script(f, &["next", "n", "r"], &[]);
    assert!(ok, "{out}");
    assert_eq!(
        out,
        "11   530           OUT  n\nout: 42\n12   900           HRS  00\nout: 42\nhalted: HRS in cell 12, after 9 instructions\n"
    );
    // step goes into it
    let (ok, out, _) = script(f, &["s"], &[]);
    assert!(ok && out == "20   199  twice    CLA  99\n", "{out}");
    // a breakpoint in the routine stops it
    let (ok, out, _) = script(f, &["b 24", "n"], &[]);
    assert!(
        ok && out.ends_with("breakpoint, cell 24\n24   630           STO  n\n"),
        "{out}"
    );
}

#[test]
fn an_inp_with_no_card_waits_for_input() {
    let (ok, out, _) = script(
        "tests/manual/add.cq",
        &["run", "input 123", "c", "input 456", "c", "c"],
        &[],
    );
    assert!(ok, "{out}");
    assert!(
        out.starts_with("INP in cell 17 is waiting for a card: input CARDS, then continue\n"),
        "{out}"
    );
    assert!(out.contains("INP in cell 18 is waiting"), "{out}");
    assert!(
        out.ends_with(
            "out: 579\nhalted: HRS in cell 23, after 7 instructions\nthe program has halted: run starts it again\n"
        ),
        "{out}"
    );
    // with --batch, a command that fails makes ekg exit 1
    let (ok, _, err) = script("tests/manual/add.cq", &["frobnicate"], &[]);
    assert!(!ok && err == "frobnicate: not a command (help lists them)\n", "{err}");
}

#[test]
fn a_profile_counts_runs_writes_and_what_never_ran() {
    let (ok, out, err) = ekg(&["--profile", SORTEM, "--", "5", "3"]);
    assert!(ok, "{err}");
    // the program's cards on standard error, the report on standard output
    assert_eq!(err, "3\n5\n4\n3\n5\n5\n");
    assert!(
        out.starts_with(&format!("{SORTEM}: halted after 128 instructions, in ")),
        "{out}"
    );
    assert!(
        out.contains("\nHottest cells:\n           5   3.91%  70   194  dv       CLA  sum\n"),
        "{out}"
    );
    assert!(
        out.contains("           2   1.56%  08   098  rdi      INP  98       ; INP tab+i (listed 000)\n"),
        "{out}"
    );
    assert!(
        out.contains("\n                      41   817           JMP  ps\n"),
        "{out}"
    );
    assert!(
        out.contains("  08  rdi      written          2  run too: code the program rewrites\n"),
        "{out}"
    );
    assert!(out.contains("  94  sum      written          7\n"), "{out}");
    assert!(
        out.contains("  99           written          5  the way back JMP leaves\n"),
        "{out}"
    );
    assert!(
        out.contains("\nListed instructions never run:\n  41   817           JMP  ps\n"),
        "{out}"
    );
    // the input on standard input, as burn reads it
    let (ok, _, err) = run(env!("CARGO_BIN_EXE_ekg"), &["--profile", SORTEM], "5\n3\n");
    assert!(ok && err == "3\n5\n4\n3\n5\n5\n", "{err}");
}

#[test]
fn a_deck_is_loaded_by_its_loader_and_disassembled() {
    let deck = dir("deck").join("add.deck");
    let (ok, _, said) = run(
        env!("CARGO_BIN_EXE_carc"),
        &[
            "tests/manual/add.cq",
            "--start-at",
            "17",
            "-o",
            deck.to_str().unwrap(),
        ],
        "",
    );
    assert!(ok, "{said}");
    let d = deck.to_str().unwrap();
    let (ok, out, _) = script(d, &["info r", "list", "b 21", "c", "p 36", "c"], &["123", "456"]);
    assert!(ok, "{out}");
    // loaded: the loader's instructions run, the program's cards in the reader, the bug on its start
    assert!(out.contains("pc 17  acc +0000  steps 27  reader [123 456]\n"), "{out}");
    assert!(
        out.contains(" > 17   034           INP  34\n   18   035           INP  35\n"),
        "{out}"
    );
    assert!(
        out.contains("breakpoint, cell 21\n21   636           STO  36\n"),
        "{out}"
    );
    assert!(out.contains("36 = 000    INP 00\n"), "{out}");
    assert!(
        out.contains("out: 579\nhalted: HRS in cell 23, after 34 instructions\n"),
        "{out}"
    );
    let (ok, out, err) = ekg(&["--profile", d, "--", "123", "456"]);
    assert!(ok && err == "579\n", "{err}");
    assert!(
        out.starts_with(&format!(
            "{d}: halted after 7 instructions (and 27 loading the deck), in "
        )),
        "{out}"
    );
}

#[test]
fn a_listing_with_errors_is_not_run() {
    let f = dir("errors").join("bad.cq");
    std::fs::write(&f, "10 CLA x\n11 HRS\n").unwrap();
    let f = f.to_str().unwrap();
    let (ok, out, err) = ekg(&[f, "-x", "run"]);
    assert!(!ok && out.is_empty(), "{out}");
    assert_eq!(
        err,
        format!("ekg: {f}:1:8: error: unknown label x\nekg: {f}: not assembled\n")
    );
    let (ok, _, err) = ekg(&["nowhere.cq"]);
    assert!(
        !ok && err == "ekg: nowhere.cq: No such file or directory\n",
        "{err}"
    );
}

#[test]
fn a_run_that_never_stops_is_stopped_at_the_limit() {
    let f = dir("forever").join("forever.cq");
    std::fs::write(&f, "10 top JMP top\n").unwrap();
    let f = f.to_str().unwrap();
    let (ok, out, _) = ekg(&["--batch", f, "--limit=1000", "-x", "c", "-xc", "-x", "info r"]);
    assert!(ok, "{out}");
    assert!(
        out.starts_with(
            "still running after 1000 instructions (--limit): continue goes on\n10   810  top      JMP  top\n"
        ),
        "{out}"
    );
    assert!(out.contains("pc 10  acc +0000  steps 2000  reader []"), "{out}");
    let (ok, out, err) = ekg(&["--profile", "--limit", "1000", f]);
    assert!(!ok && err.contains("still running after 1000 instructions"), "{err}");
    assert!(
        out.contains("        1000 100.00%  10   810  top      JMP  top\n"),
        "{out}"
    );
}

#[test]
fn help_version_and_usage_errors_are_gnu_style() {
    let (ok, out, err) = ekg(&["--help"]);
    assert!(
        ok && err.is_empty() && out.starts_with("Usage: ekg [OPTION]... FILE [INPUT]...\n"),
        "{out}"
    );
    assert!(out.contains("\nCommands"), "{out}");
    let (ok, out, _) = ekg(&["--version"]);
    assert!(
        ok && out.starts_with(&format!("ekg (cardiaq) {}\n", env!("CARGO_PKG_VERSION"))),
        "{out}"
    );
    assert!(out.contains("License MIT"), "{out}");
    let usage = |args: &[&str], said: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_ekg")).args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(
            String::from_utf8(out.stderr).unwrap(),
            format!("ekg: {said}\nTry 'ekg --help' for more information.\n")
        );
    };
    usage(&["--frob", SORTEM], "unrecognized option '--frob'");
    usage(&[], "no program: give it a FILE");
    usage(&[SORTEM, "x"], "not a card: x (a card holds a number from -999 to 999)");
}
