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
    let deck = dir("add").join("add.deck");
    let (ok, _, said) = carc(&[
        "tests/manual/add.cardiac",
        "--start",
        "17",
        "-o",
        deck.to_str().unwrap(),
    ]);
    assert!(ok && said.is_empty(), "{said}");
    let text = std::fs::read_to_string(&deck).unwrap();
    assert!(
        text.starts_with("# add.deck: add.cardiac on cards, for the loader; it starts at 17\n"),
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

#[test]
fn the_deck_goes_beside_the_listing_and_starts_where_it_says() {
    let d = dir("beside");
    let listing = d.join("count.cardiac");
    std::fs::write(&listing, "; Start at 10.\n10 start OUT n\n11 HRS 00\n30 n DATA 5\n").unwrap();
    let (ok, _, said) = carc(&[listing.to_str().unwrap()]);
    assert!(ok, "{said}");
    let deck = std::fs::read_to_string(d.join("count.deck")).unwrap();
    assert!(
        deck.ends_with("\n002\n800\n010\n530\n011\n900\n030\n005\n002\n810\n"),
        "{deck}"
    );
}

#[test]
fn errors_are_reported_and_no_deck_is_written() {
    let d = dir("errors");
    let listing = d.join("bad.cardiac");
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
    assert!(!d.join("bad.deck").exists());
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
        let out = dir("manual").join(p.file_name().unwrap()).with_extension("deck");
        let (ok, _, said) = carc(&[p.to_str().unwrap(), "-o", out.to_str().unwrap()]);
        assert!(ok && !said.contains("error"), "{}: {said}", p.display());
    }
}
