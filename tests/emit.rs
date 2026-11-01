//! `heartburn listing`: programs written out in Brian Stuart's six columns read back as the same program. From
//! the manual's listings, from tapes (each of a program's tapes), and from CARDIASM.
use std::path::{Path, PathBuf};
use std::process::Command;

use heartburn::cardiac;
use heartburn::listing::{self, Listing, Notes, Read};

fn heartburn(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_heartburn-tools"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .unwrap();
    let said = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "heartburn {args:?}: {said}");
    String::from_utf8(out.stdout).unwrap()
}

/// A directory of its own for a test's files.
fn dir(test: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("heartburn-emit-{test}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn read(path: &Path) -> Read {
    listing::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn cells(r: &Read) -> Vec<(usize, i32)> {
    let Listing::Memory(mut c) = r.listing.clone() else {
        panic!("a memory listing")
    };
    c.sort();
    c
}

/// Each of the manual's memory listings, in six columns, is the same program: the same cells, the start the
/// listing names, its comments kept; and it prints the same.
#[test]
fn the_manuals_programs_in_six_columns() {
    let d = dir("manual");
    for (name, start, input) in [
        ("add", "17", vec!["123", "456"]),
        ("counting", "20", vec![]),
        ("countdown", "20", vec![]),
        ("multiply", "07", vec!["25", "5"]),
        ("reverse", "15", vec!["269"]),
        ("double-precision-add", "50", vec!["163", "742", "258", "500"]),
        ("nim", "01", vec![]),
    ] {
        let src = format!("tests/manual/{name}.cq");
        let out = d.join(format!("{name}.cq"));
        heartburn(&["listing", &src, "--start", start, "-o", out.to_str().unwrap()]);
        let (was, now) = (read(Path::new(&src)), read(&out));
        assert_eq!(cells(&now), cells(&was), "{name}");
        assert_eq!(now.start, Some(start.parse().unwrap()), "{name}");
        assert!(now.warnings.is_empty(), "{name}: {:?}", now.warnings);
        let mut args = vec!["run", &src, "--start", start, "--"];
        args.extend(&input);
        let printed = heartburn(&args);
        let o = out.to_str().unwrap();
        let mut args = vec!["run", o, "--"];
        args.extend(&input);
        assert_eq!(heartburn(&args), printed, "{name}");
    }
    let six = std::fs::read_to_string(d.join("reverse.cq")).unwrap();
    assert!(
        six.contains("15\t039\t\tINP\t39\tRead \"abc\" into cell 39.\n"),
        "{six}"
    );
    assert!(six.starts_with("Start at 15.\n"));
}

/// A program in two phases: each tape becomes a listing, and their tapes run as the first ones did.
#[test]
fn tapes_to_listings_and_back() {
    let d = dir("tapes");
    let t = |n: &str| d.join(n).to_str().unwrap().to_string();
    heartburn(&["deck", "tests/sortem/sortem-1.cq", "-o", &t("sortem.deck")]);
    heartburn(&["deck", "tests/sortem/sortem-2.cq", "-o", &t("sortem.2.deck")]);
    heartburn(&["listing", &t("sortem.deck"), "-o", &t("again.cq")]);
    assert!(d.join("again.2.cq").is_file());
    heartburn(&["deck", &t("again.cq"), "-o", &t("again.deck")]);
    heartburn(&["deck", &t("again.2.cq"), "-o", &t("again.2.deck")]);
    let input: Vec<String> = (1..=35).rev().map(|n| n.to_string()).collect();
    let run = |tape: &str| {
        let mut args = vec!["run", tape, "--"];
        args.extend(input.iter().map(String::as_str));
        heartburn(&args)
    };
    assert_eq!(run(&t("again.deck")), run(&t("sortem.deck")));
    // the same cards, too, the cells in the order of their addresses (the sortem listings list them so)
    for n in ["sortem.deck", "sortem.2.deck"] {
        let again = n.replace("sortem", "again");
        assert_eq!(
            std::fs::read_to_string(d.join(again)).unwrap(),
            std::fs::read_to_string(d.join(n)).unwrap()
        );
    }
    // data stays data: phase 2's variables
    let two = std::fs::read_to_string(d.join("again.2.cq")).unwrap();
    assert!(two.contains("36\t035\t\tDATA\t035\n"), "{two}");
}

/// A deck the loader hands over from: its cells, its start, and its input left in the reader.
#[test]
fn what_a_deck_loads() {
    let deck = [2, 800, 10, 20, 11, 520, 12, 900, 2, 810, 42];
    let l = listing::from_deck(&deck).unwrap();
    assert_eq!(
        (l.cells, l.start, l.left),
        (vec![(10, 20), (11, 520), (12, 900)], Some(10), vec![42])
    );
    // a deck that ends with the manual's BLANK card has no start
    let l = listing::from_deck(&[2, 800, 10, 20]).unwrap();
    assert_eq!((l.cells, l.start), (vec![(10, 20)], None));
}

/// Code is what the program can run: a routine called by JMP that reads its return address (CLA 99) returns to
/// the cell after the JMP; a JMP that loops doesn't, and the cells after it are data.
#[test]
fn code_and_data() {
    // 10 JMP 20 (a call), 11 OUT 30, 12 JMP 10 (a loop), 13 data; 20 STO 31, 21 CLA 99, 22 STO 23, 23 (the
    // return, written by the routine); 30 data
    let cells = [
        (10, 820),
        (11, 530),
        (12, 810),
        (13, 7),
        (20, 631),
        (21, 199),
        (22, 623),
        (23, 0),
        (30, 5),
    ];
    let six = listing::six_columns(&cells, Some(10), &Notes::default());
    let ops: Vec<&str> = six.lines().skip(2).map(|l| l.split('\t').nth(3).unwrap()).collect();
    assert_eq!(ops, ["JMP", "OUT", "JMP", "DATA", "STO", "CLA", "STO", "INP", "DATA"]);
    let back = listing::parse(&six).unwrap();
    assert_eq!(back.listing, Listing::Memory(cells.to_vec()));
    let m = listing::memory(&cells);
    let mut out = vec![];
    cardiac::run_from(&m, 10, &[], &mut std::iter::empty(), &mut out, 50);
    assert_eq!(&out[..3], [5, 5, 5]);
}

#[test]
fn cardiasm_keeps_its_names() {
    let d = dir("cardiasm");
    let src = d.join("add.cardiasm");
    std::fs::write(
        &src,
        "DEF a 0\nDEF b 0\nstart: INP a\nINP b\nCLA a\nADD b\nSTO a\nOUT a\nHRS 0\n",
    )
    .unwrap();
    let out = d.join("add.cq");
    heartburn(&["listing", src.to_str().unwrap(), "-o", out.to_str().unwrap()]);
    let six = std::fs::read_to_string(&out).unwrap();
    assert!(six.starts_with("Start at 03.\n"), "{six}");
    assert!(six.contains("03\t098\tstart\tINP\ta\n"), "{six}");
    assert!(six.contains("98\t000\ta\tDATA\t000\n"), "{six}");
    let printed = heartburn(&["run", out.to_str().unwrap(), "--", "2", "3"]);
    assert_eq!(printed, "5\n");
}

/// Brian Stuart's push: a routine that returns by running on into cell 99, where the JMP that called it left the
/// return. The cell after the call is code.
#[test]
fn a_routine_that_runs_into_99_returns() {
    // 10 JMP 97 (a call), 11 OUT 30, 12 HRS; 97 STO 30, 98 CLA 30 (and on into 99); 30 data
    let cells = [(10, 897), (11, 530), (12, 900), (30, 5), (97, 630), (98, 130)];
    let six = listing::six_columns(&cells, Some(10), &Notes::default());
    let ops: Vec<&str> = six.lines().skip(2).map(|l| l.split('\t').nth(3).unwrap()).collect();
    assert_eq!(ops, ["JMP", "OUT", "HRS", "DATA", "STO", "CLA"]);
}
