//! `burn`: it runs decks and nothing else, a deck known by what's in it (the loader's 002, 800), not its name. A deck made of a listing (by heartburn-tools, as CARDIAC Code's Export
//! makes one) runs; a program in two decks goes on into NAME.2.deck; input comes after -- or on standard input;
//! a listing is refused.
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn dir(test: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("heartburn-burn-{test}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn tools(args: &[&str]) {
    let out = Command::new(env!("CARGO_BIN_EXE_heartburn-tools"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "heartburn-tools {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `burn ARGS`, `stdin` on its standard input: (it succeeded, what it printed, what it said on standard error).
fn burn(args: &[&str], stdin: &str) -> (bool, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_burn"))
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

#[test]
fn burn_runs_a_deck_with_input_after_dashes_or_piped() {
    let d = dir("add");
    let deck = d.join("add.deck");
    tools(&[
        "deck",
        "tests/manual/add.cardiac",
        "--start",
        "17",
        "-o",
        deck.to_str().unwrap(),
    ]);
    let (ok, printed, said) = burn(&[deck.to_str().unwrap(), "--", "123", "456"], "");
    assert!(ok && printed == "579\n", "{printed} {said}");
    assert!(said.contains("halted after"), "{said}");
    let (ok, printed, _) = burn(&[deck.to_str().unwrap()], "123\n456\n");
    assert!(ok && printed == "579\n", "{printed}");
}

#[test]
fn burn_goes_on_into_the_next_deck() {
    let d = dir("sortem");
    let t = |n: &str| d.join(n).to_str().unwrap().to_string();
    tools(&["deck", "tests/sortem/sortem-1.cardiac", "-o", &t("sortem.deck")]);
    tools(&["deck", "tests/sortem/sortem-2.cardiac", "-o", &t("sortem.2.deck")]);
    let input: String = (1..=35).rev().map(|n| format!("{n} ")).collect();
    let (ok, printed, said) = burn(&[&t("sortem.deck")], &input);
    assert!(ok, "{said}");
    let cards: Vec<i32> = printed.lines().map(|l| l.parse().unwrap()).collect();
    assert_eq!(&cards[..35], (1..=35).collect::<Vec<_>>().as_slice(), "sorted");
    assert_eq!(&cards[35..], [18, 1, 35, 18], "mean, min, max, median");
}

#[test]
fn burn_looks_inside_not_at_the_name() {
    // a listing is refused for what it is, not for its name
    let (ok, _, said) = burn(&["tests/manual/add.cardiac"], "");
    assert!(!ok && said.contains("not a CARDIAC deck"), "{said}");
    // a deck runs whatever it is called
    let d = dir("names");
    let odd = d.join("add.cards");
    tools(&[
        "deck",
        "tests/manual/add.cardiac",
        "--start",
        "17",
        "-o",
        odd.to_str().unwrap(),
    ]);
    let (ok, printed, said) = burn(&[odd.to_str().unwrap(), "--", "1", "2"], "");
    assert!(ok && printed == "3\n", "{printed} {said}");
    // cards that don't start with the loader aren't a deck
    let bare = d.join("bare.deck");
    std::fs::write(&bare, "100\n200\n").unwrap();
    let (ok, _, said) = burn(&[bare.to_str().unwrap()], "");
    assert!(!ok && said.contains("not a CARDIAC deck"), "{said}");
    let (ok, printed, _) = burn(&["bench", "--", "1"], "");
    assert!(!ok && printed.is_empty());
}

/// `burn ARGS`, `stdin` on its standard input: its exit status, what it printed, what it said on standard error.
fn burn_status(args: &[&str], stdin: &str) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_burn"))
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
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn add_deck(test: &str) -> String {
    let deck = dir(test).join("add.deck");
    tools(&[
        "deck",
        "tests/manual/add.cardiac",
        "--start",
        "17",
        "-o",
        deck.to_str().unwrap(),
    ]);
    deck.to_str().unwrap().to_string()
}

#[test]
fn help_and_version_go_to_standard_output() {
    for a in [&["--help"][..], &["-h"], &["help"]] {
        let (code, printed, said) = burn_status(a, "");
        assert!(code == 0 && printed.starts_with("Usage: burn [OPTION]... DECK [--] [INPUT]...\n") && said.is_empty());
        assert!(printed.contains("burn bench"));
    }
    let (code, printed, _) = burn_status(&["--version"], "");
    assert_eq!(code, 0);
    assert_eq!(
        printed.lines().next(),
        Some(format!("burn (cardiaq) {}", env!("CARGO_PKG_VERSION")).as_str())
    );
}

#[test]
fn command_line_mistakes_exit_2_and_a_deck_that_cant_run_1() {
    let deck = add_deck("mistakes");
    for (args, why) in [
        (&["--frob"][..], "unrecognized option '--frob'".to_string()),
        (&["-x", "a.deck"], "invalid option -- 'x'".to_string()),
        (&[], "missing deck operand".to_string()),
        (&["bench", "--", "1"], "extra operand '--'".to_string()),
        (
            &[deck.as_str(), "--", "1x"],
            "invalid input: not a card: 1x (a card holds a number from -999 to 999)".to_string(),
        ),
    ] {
        let (code, printed, said) = burn_status(args, "");
        assert_eq!((code, printed.as_str()), (2, ""), "{args:?}");
        assert_eq!(said, format!("burn: {why}\nTry 'burn --help' for more information.\n"));
    }
    let (code, _, said) = burn_status(&["nowhere.deck"], "");
    assert_eq!(
        (code, said.as_str()),
        (1, "burn: nowhere.deck: No such file or directory\n")
    );
    let (code, _, said) = burn_status(&["tests"], "");
    assert_eq!((code, said.as_str()), (1, "burn: tests: Is a directory\n"));
    let (code, _, said) = burn_status(&["tests/manual/add.cardiac"], "");
    assert!(
        code == 1 && said.starts_with("burn: tests/manual/add.cardiac: not a CARDIAC deck"),
        "{said}"
    );
}

#[test]
fn the_input_after_the_deck_with_or_without_dashes() {
    let deck = add_deck("input");
    // after the deck everything is input, a minus sign and all
    let (code, printed, _) = burn_status(&[&deck, "-5", "7"], "");
    assert_eq!((code, printed.as_str()), (0, "2\n"));
    let (code, printed, _) = burn_status(&["--", &deck, "--", "1", "2"], "");
    assert_eq!((code, printed.as_str()), (0, "3\n"));
    // -- and nothing after it: no input, and standard input isn't read
    let (code, printed, said) = burn_status(&[&deck, "--"], "1 2\n");
    assert!(
        code == 0 && printed.is_empty() && said.contains("out of cards"),
        "{said}"
    );
}

#[test]
fn a_deck_from_standard_input() {
    let deck = std::fs::read_to_string(add_deck("stdin")).unwrap();
    let (code, printed, said) = burn_status(&["-", "--", "123", "456"], &deck);
    assert_eq!((code, printed.as_str()), (0, "579\n"), "{said}");
    // the input after the deck's cards, as on the reader
    let (code, printed, _) = burn_status(&["-"], &format!("{deck}123\n456\n"));
    assert_eq!((code, printed.as_str()), (0, "579\n"));
    let (code, _, said) = burn_status(&["-"], "100\n");
    assert!(
        code == 1 && said.starts_with("burn: standard input: not a CARDIAC deck"),
        "{said}"
    );
}
