//! The 1968 manual's ten programs, as printed (tests/manual, copied from the transcriptions in cardiac-code), run
//! as listings: started where the manual says, on its example input, they print what it says they print.
use std::process::Command;

use heartburn::cardiac::{self, Stop};
use heartburn::listing::{self, Listing};

/// `heartburn run ARGS`: what it punched, one card a line, and what it said on standard error.
fn run(args: &[&str]) -> (Vec<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_heartburn"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("run")
        .args(args)
        .output()
        .unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "heartburn run {args:?}: {stderr}");
    let punched = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| l.parse().unwrap())
        .collect();
    (punched, stderr)
}

fn punched(args: &[&str]) -> Vec<i32> {
    run(args).0
}

#[test]
fn no_1_adds() {
    assert_eq!(
        punched(&["tests/manual/add.txt", "--start", "17", "--", "123", "456"]),
        [579]
    );
}

#[test]
fn no_2_counts_to_five() {
    assert_eq!(
        punched(&["tests/manual/counting.txt", "--start", "20"]),
        [1, 2, 3, 4, 5]
    );
}

#[test]
fn no_3_counts_on() {
    // it never stops: run it a while
    let Listing::Memory(cells) = listing::parse(include_str!("manual/counting-loop.txt"))
        .unwrap()
        .listing
    else {
        panic!("a memory listing")
    };
    let mut out = vec![];
    let end = cardiac::run_from(
        &listing::memory(&cells),
        21,
        &[],
        &mut std::iter::empty(),
        &mut out,
        1 + 4 * 6,
    );
    assert_eq!((end.stop, out), (Stop::Limit, vec![1, 2, 3, 4, 5, 6]));
}

#[test]
fn no_4_counts_down() {
    assert_eq!(
        punched(&["tests/manual/countdown.txt", "--start", "20"]),
        [-3, -2, -1, 0]
    );
}

#[test]
fn no_5_multiplies() {
    assert_eq!(
        punched(&["tests/manual/multiply.txt", "--start", "07", "--", "25", "5"]),
        [125]
    );
}

#[test]
fn no_6_reverses() {
    assert_eq!(
        punched(&["tests/manual/reverse.txt", "--start", "15", "--", "269"]),
        [962]
    );
}

#[test]
fn no_7_loads_itself_from_cards() {
    // the BLANK card stops the loader; the operator starts the program at 10
    assert_eq!(
        punched(&[
            "tests/manual/bootstrap-addition.txt",
            "--start",
            "10",
            "--",
            "123",
            "456"
        ]),
        [579]
    );
}

#[test]
fn no_8_adds_in_double_precision() {
    // cell 94 is 8__ as printed: the subroutine writes it
    let (p, said) = run(&[
        "tests/manual/double-precision-add.txt",
        "--start",
        "50",
        "--",
        "163",
        "742",
        "258",
        "500",
    ]);
    assert_eq!(p, [422, 242]);
    assert!(said.contains("cell 94 holds 8__"), "{said}");
}

#[test]
fn no_9_plays_nim() {
    assert_eq!(punched(&["tests/manual/nim.txt", "--start", "01"]), [19]);
    assert_eq!(punched(&["tests/manual/nim.txt", "--start", "00", "--", "517"]), [34]);
    assert_eq!(punched(&["tests/manual/nim.txt", "--start", "00", "--", "513"]), [30]);
}

#[test]
fn no_10_plays_better_nim() {
    // N(start) as printed: the pile goes in a listing of its own, loaded over the program
    let (p, said) = run(&[
        "tests/manual/improved-nim.txt",
        "tests/manual/pile-13.txt",
        "--start",
        "52",
        "--",
        "1",
    ]);
    assert_eq!((p, said.contains("warning")), (vec![12, 3, 9], false));
    assert_eq!(
        punched(&[
            "tests/manual/improved-nim.txt",
            "tests/manual/pile-15.txt",
            "--start",
            "53"
        ]),
        [15, 3, 12]
    );
}

#[test]
fn a_listing_runs_the_same_from_its_tape() {
    let Listing::Memory(cells) = listing::parse(include_str!("manual/reverse.txt")).unwrap().listing else {
        panic!("a memory listing")
    };
    let tape = heartburn::tape::Tape::one(listing::deck(15, &cells).unwrap());
    assert_eq!(tape.run(&[269], 1000).unwrap().0, [962]);
}

/// sortem (cardiac-code keeps it with the other sample programs; tests/sortem is a copy, as a program big enough to
/// need two tapes) sorts and summarises its numbers in one tape and in two.
#[test]
fn sortem_sorts_in_one_phase_and_in_two() {
    let deck = |text: &str| {
        let Listing::Memory(cells) = listing::parse(text).unwrap().listing else {
            panic!("a memory listing")
        };
        listing::deck(3, &cells).unwrap()
    };
    let one = heartburn::tape::Tape::one(deck(include_str!("sortem/sortem.lst")));
    assert_eq!(one.run(&[31, 7], 100_000).unwrap().0, [7, 31, 19, 7, 31, 31]);
    let two = heartburn::tape::Tape {
        phases: vec![
            deck(include_str!("sortem/sortem-1.lst")),
            deck(include_str!("sortem/sortem-2.lst")),
        ],
        slots: vec![],
    };
    let input: Vec<i32> = (1..=35).rev().collect();
    let (out, stop, _) = two.run(&input, 1_000_000).unwrap();
    let mut want: Vec<i32> = (1..=35).collect();
    // then mean (630 / 35), min, max, median
    want.extend([18, 1, 35, 18]);
    assert_eq!((out, stop), (want, Stop::Halt));
}
