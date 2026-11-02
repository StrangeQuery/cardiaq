//! pace and lub as a user runs them: samples in, a tape, the samples a CARDIAC decodes from it out.
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(prog: &str, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(prog)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

/// A rising tone, loud enough to need several steps, with a stretch at the top of 16 bits.
fn tone() -> Vec<i16> {
    (0..3000)
        .map(|i| match i {
            1000..=1100 => 32767,
            _ => ((i as f64 * i as f64 / 2e5).sin() * 20000.0) as i16,
        })
        .collect()
}

#[test]
fn lub_plays_what_pace_codes() {
    let xs = tone();
    let raw: Vec<u8> = xs.iter().flat_map(|s| s.to_le_bytes()).collect();
    let tape = run(env!("CARGO_BIN_EXE_pace"), &[], &raw);
    assert!(tape.status.success(), "{}", String::from_utf8_lossy(&tape.stderr));
    let (cards, expected) = heartburn::audio::encode(&xs);
    let text: String = cards.iter().map(|c| format!("{c}\n")).collect();
    assert_eq!(String::from_utf8(tape.stdout.clone()).unwrap(), text);

    let played = run(env!("CARGO_BIN_EXE_lub"), &[], &tape.stdout);
    assert!(played.status.success(), "{}", String::from_utf8_lossy(&played.stderr));
    let want: Vec<u8> = expected.iter().flat_map(|s| s.to_le_bytes()).collect();
    assert_eq!(played.stdout, want);
}

#[test]
fn lub_says_when_the_tape_has_no_end() {
    let xs = tone();
    let (cards, _) = heartburn::audio::encode(&xs);
    let text: String = cards[..cards.len() - 1].iter().map(|c| format!("{c}\n")).collect();
    let played = run(env!("CARGO_BIN_EXE_lub"), &[], text.as_bytes());
    assert_eq!(played.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&played.stderr),
        "lub: standard input ended without its end card (-1)\n"
    );
}

#[test]
fn lub_says_which_line_isnt_a_card() {
    let played = run(env!("CARGO_BIN_EXE_lub"), &[], b"0\n6\n1234\n");
    assert_eq!(played.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&played.stderr),
        "lub: standard input: line 3: not a card: 1234 (a card holds a number from -999 to 999)\n"
    );
}

#[test]
fn pace_refuses_half_a_sample() {
    let tape = run(env!("CARGO_BIN_EXE_pace"), &[], &[1, 2, 3]);
    assert_eq!(tape.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&tape.stderr),
        "pace: standard input: an odd number of bytes: not 16-bit samples\n"
    );
}
