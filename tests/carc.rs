//! `carc`: a listing assembled into a deck that `burn` runs; what is wrong reported compiler style, and no deck
//! written while there are errors.
use std::path::PathBuf;
use std::process::Command;

fn dir(test: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("heartburn-carc-{test}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `EXE ARGS`: (it succeeded, what it printed, what it said on standard error).
fn run(exe: &str, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(exe)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn carc(args: &[&str]) -> (bool, String, String) {
    run(env!("CARGO_BIN_EXE_carc"), args)
}

#[test]
fn carc_makes_a_deck_burn_runs() {
    let deck = dir("add").join("add.cqb");
    let (ok, _, said) = carc(&[
        "tests/manual/add.cq",
        "--start-at",
        "17",
        "-o",
        deck.to_str().unwrap(),
    ]);
    assert!(ok && said.is_empty(), "{said}");
    let text = std::fs::read_to_string(&deck).unwrap();
    assert!(
        text.starts_with("# add.cqb: add.cq on cards, for the loader; it starts at 17\n"),
        "{text}"
    );
    assert!(
        text.ends_with("002\n800\n017\n034\n018\n035\n019\n134\n020\n235\n021\n636\n022\n536\n023\n900\n002\n817\n")
    );
    let (ok, printed, said) = run(
        env!("CARGO_BIN_EXE_burn"),
        &[deck.to_str().unwrap(), "--", "123", "456"],
    );
    assert!(ok && printed == "579\n", "{printed} {said}");
}

/// A program in two decks carc made, each with carc's `#` lines at the top: burn goes on into the second (it once
/// took the comments for the one-file form and stopped after the first deck, printing nothing).
#[test]
fn burn_goes_on_into_the_second_deck_carc_made() {
    let d = dir("sortem");
    let t = |n: &str| d.join(n).to_str().unwrap().to_string();
    for (listing, deck) in [("sortem-1", "sortem.cqb"), ("sortem-2", "sortem.2.cqb")] {
        let (ok, _, said) = carc(&[&format!("tests/sortem/{listing}.cq"), "-o", &t(deck)]);
        assert!(ok, "{said}");
    }
    assert!(std::fs::read_to_string(t("sortem.cqb")).unwrap().starts_with('#'));
    let input: Vec<String> = (1..=35).rev().map(|n| n.to_string()).collect();
    let mut args = vec![t("sortem.cqb"), "--".to_string()];
    args.extend(input);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, printed, said) = run(env!("CARGO_BIN_EXE_burn"), &args);
    assert!(ok, "{said}");
    let cards: Vec<i32> = printed.lines().map(|l| l.parse().unwrap()).collect();
    assert_eq!(&cards[..35], (1..=35).collect::<Vec<_>>().as_slice(), "sorted");
    assert_eq!(&cards[35..], [18, 1, 35, 18], "mean, min, max, median");
}

#[test]
fn the_deck_goes_beside_the_listing_and_starts_where_it_says() {
    let d = dir("beside");
    let listing = d.join("count.cq");
    std::fs::write(&listing, "; Start at 10.\n10 start OUT n\n11 HRS 00\n30 n DATA 5\n").unwrap();
    let (ok, _, said) = carc(&[listing.to_str().unwrap()]);
    assert!(ok, "{said}");
    let deck = std::fs::read_to_string(d.join("count.cqb")).unwrap();
    assert!(
        deck.ends_with("\n002\n800\n010\n530\n011\n900\n030\n005\n002\n810\n"),
        "{deck}"
    );
}

#[test]
fn errors_are_reported_and_no_deck_is_written() {
    let d = dir("errors");
    let listing = d.join("bad.cq");
    std::fs::write(
        &listing,
        "10 CLA x\n11 199 CLA 99\n12 CLA nowhere\n\u{201c}q\u{201d} 13 CLA 99\n",
    )
    .unwrap();
    let (ok, _, said) = carc(&[listing.to_str().unwrap()]);
    let f = listing.to_str().unwrap();
    assert!(!ok);
    assert!(said.contains(&format!("{f}:1:8: error: unknown label x\n")), "{said}");
    assert!(
        said.contains(&format!("{f}:3:8: error: unknown label nowhere\n")),
        "{said}"
    );
    assert!(!said.contains(":2:"), "{said}");
    assert!(!d.join("bad.cqb").exists());
    // a warning alone doesn't stop it
    std::fs::write(&listing, "10 199 CLA 20\n20 DATA 1\n").unwrap();
    let (ok, _, said) = carc(&[listing.to_str().unwrap()]);
    assert!(ok, "{said}");
    assert_eq!(
        said,
        format!("{f}:1:4: warning: contents +199, but CLA 20 is +120: loading the contents\n")
    );
}

#[test]
fn every_manual_listing_assembles() {
    for f in std::fs::read_dir("tests/manual").unwrap() {
        let p = f.unwrap().path();
        let out = dir("manual").join(p.file_name().unwrap()).with_extension("cqb");
        let (ok, _, said) = carc(&[p.to_str().unwrap(), "-o", out.to_str().unwrap()]);
        assert!(ok && !said.contains("error"), "{}: {said}", p.display());
    }
}

/// `carc ARGS`, `stdin` on its standard input: (its exit status, what it printed, what it said on standard error).
fn carc_in(args: &[&str], stdin: &str) -> (i32, String, String) {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_carc"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
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

#[test]
fn help_and_version_go_to_standard_output() {
    for a in ["--help", "-h", "--he"] {
        let (code, printed, said) = carc_in(&[a], "");
        assert!(code == 0 && printed.starts_with("Usage: carc [OPTION]... FILE\n") && said.is_empty());
    }
    let (code, printed, _) = carc_in(&["--version"], "");
    assert_eq!(code, 0);
    assert_eq!(
        printed.lines().next(),
        Some(format!("carc (cardiaq) {}", env!("CARGO_PKG_VERSION")).as_str())
    );
    assert!(printed.contains("MIT"), "{printed}");
}

#[test]
fn command_line_mistakes_are_reported_the_gnu_way_and_exit_2() {
    let try_help = "Try 'carc --help' for more information.\n";
    for (args, why) in [
        (&["--frob", "x"][..], "unrecognized option '--frob'"),
        (&["-x"], "invalid option -- 'x'"),
        (&["x", "-o"], "option requires an argument -- 'o'"),
        (&["x", "--output"], "option '--output' requires an argument"),
        (&["--help=1"], "option '--help' doesn't allow an argument"),
        (&[], "no input file"),
        (&["a", "b"], "extra operand 'b'"),
        (
            &["--start-at", "100", "x"],
            "invalid cell '100' for '--start-at': a cell is 00 to 99",
        ),
    ] {
        let (code, printed, said) = carc_in(args, "");
        assert_eq!((code, printed.as_str()), (2, ""), "{args:?}");
        assert_eq!(said, format!("carc: {why}\n{try_help}"), "{args:?}");
    }
    // a file that isn't there is not a mistake on the command line
    let (code, _, said) = carc_in(&["nowhere.cq"], "");
    assert_eq!(
        (code, said.as_str()),
        (1, "carc: nowhere.cq: No such file or directory\n")
    );
}

#[test]
fn options_long_short_bundled_or_after_the_file() {
    let d = dir("options");
    let want = "002\n800\n017\n034\n018\n035\n019\n134\n020\n235\n021\n636\n022\n536\n023\n900\n002\n817\n";
    let out = |n: &str| d.join(n).to_str().unwrap().to_string();
    let add = "tests/manual/add.cq".to_string();
    for (n, args) in [
        (
            "a.cqb",
            vec![
                add.clone(),
                "--start-at=17".into(),
                format!("--output={}", out("a.cqb")),
            ],
        ),
        (
            "b.cqb",
            vec!["--start-at=17".into(), "-o".into(), out("b.cqb"), add.clone()],
        ),
        (
            "c.cqb",
            vec!["--st".into(), "17".into(), add.clone(), format!("-o{}", out("c.cqb"))],
        ),
    ] {
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        let (code, _, said) = carc_in(&a, "");
        assert_eq!(code, 0, "{a:?}: {said}");
        assert!(std::fs::read_to_string(out(n)).unwrap().ends_with(want), "{a:?}");
    }
    // after --, a name starting with - is a file
    std::fs::write(d.join("-odd.cq"), "10 HRS 00\n").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_carc"))
        .current_dir(&d)
        .args(["--", "-odd.cq"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(d.join("-odd.cqb").exists());
}

#[test]
fn a_listing_from_standard_input_and_a_deck_to_standard_output() {
    let listing = std::fs::read_to_string("tests/manual/add.cq").unwrap();
    let (code, printed, said) = carc_in(&["-", "--start-at", "17"], &listing);
    assert_eq!(code, 0, "{said}");
    assert!(
        printed.starts_with("# <stdin> on cards, for the loader; it starts at 17\n"),
        "{printed}"
    );
    assert!(printed.ends_with("\n002\n817\n"), "{printed}");
    // what is wrong is reported as <stdin>
    let (code, printed, said) = carc_in(&["-"], "10 CLA x\n");
    assert_eq!((code, printed.as_str()), (1, ""));
    assert_eq!(
        said,
        "<stdin>:1:8: error: unknown label x\ncarc: <stdin>: not assembled: no deck written\n"
    );
    // -o - for a file
    let (code, printed, _) = carc_in(&["tests/manual/add.cq", "--start-at", "17", "-o", "-"], "");
    assert_eq!(code, 0);
    assert!(
        printed.starts_with("# add.cq on cards, for the loader; it starts at 17\n"),
        "{printed}"
    );
}
