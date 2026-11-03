//! dub plays two channels of audio, left and right, each decoded by its own CARDIAC. (lub plays one channel.) The
//! decoding is `heartburn::audio::Stereo`: two CARDIACs running the decoder, `audio/decode.cq`, taking turns one
//! block at a time on a single thread. dub only reads the two tracks and writes the samples to standard output,
//! left then right, which makes a stereo stream.
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, IsTerminal, Write};

use heartburn::audio;
use heartburn::tape;

#[path = "../getopt.rs"]
mod getopt;

const PROG: &str = "dub";

fn usage() -> &'static str {
    "\
Usage: dub [OPTION]... LEFT RIGHT
Decode two channels of audio, each on its own CARDIAC, and output them as one stereo stream as they are decoded.

LEFT and RIGHT are the tracks for the left and right channels, made by xtrack, with one card per line. One of
them can be -, which means standard input. The samples go to standard output as raw signed 16-bit little-endian
numbers, left then right, at the sample rate the audio was encoded at. To play them with ffplay:

  dub song.left.track song.right.track | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout stereo -

  -h, --help     display this help and exit
      --version  output version information and exit

The exit status is 0 if both tracks were played to the end, or if the player stopped reading. It is 1 if a
track can't be read, if the decoder can't decode it, or if the two tracks are different lengths, and 2 if
there is a mistake on the command line.
"
}

fn die(why: impl std::fmt::Display) -> ! {
    getopt::die(PROG, why)
}

/// The name a track is called by in messages.
fn shown(name: &str) -> String {
    if name == "-" {
        "standard input".to_string()
    } else {
        name.to_string()
    }
}

/// A track's cards, a line at a time, read as the CARDIAC asks for them.
fn cards(name: &str) -> Box<dyn Iterator<Item = i32>> {
    let reader: Box<dyn BufRead> = if name == "-" {
        Box::new(io::stdin().lock())
    } else {
        Box::new(BufReader::new(
            File::open(name).unwrap_or_else(|e| die(format!("{name}: {}", getopt::os_error(&e)))),
        ))
    };
    let at = shown(name);
    Box::new(reader.lines().enumerate().flat_map(move |(n, line)| {
        let line = line.unwrap_or_else(|e| die(format!("{at}: {}", getopt::os_error(&e))));
        tape::values(&line).unwrap_or_else(|e| die(format!("{at}: line {}: {e}", n + 1)))
    }))
}

/// The samples written; a player that stops reading is the end, not an error.
fn written(r: io::Result<()>) {
    match r {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => std::process::exit(0),
        Err(e) => die(format!("standard output: {}", getopt::os_error(&e))),
    }
}

fn main() {
    let opts = [
        getopt::Opt {
            short: Some('h'),
            long: "help",
            arg: false,
        },
        getopt::Opt {
            short: None,
            long: "version",
            arg: false,
        },
    ];
    let (given, operands) = getopt::parse(PROG, &opts, std::env::args().skip(1), false);
    match given.first().map(|(o, _)| *o) {
        Some("help") => return print!("{}", usage()),
        Some(_) => return getopt::version(PROG),
        None => {}
    }
    let (left, right) = match &operands[..] {
        [l, r] if l == "-" && r == "-" => getopt::usage_error(PROG, "only one track can be standard input"),
        [l, r] => (l.as_str(), r.as_str()),
        [] | [_] => getopt::usage_error(PROG, "two tracks: LEFT and RIGHT"),
        [_, _, extra, ..] => getopt::usage_error(PROG, format!("extra operand '{extra}'")),
    };
    if io::stdout().is_terminal() {
        getopt::usage_error(
            PROG,
            "standard output is a terminal: pipe the samples to a player, or to a file",
        )
    }
    let names = [shown(left), shown(right)];
    let mut stereo = audio::Stereo::new(cards(left), cards(right));
    let mut out = BufWriter::new(io::stdout().lock());
    loop {
        match stereo.next_block() {
            Ok(Some(block)) => {
                let bytes: Vec<u8> = block.iter().flatten().flat_map(|s| s.to_le_bytes()).collect();
                written(out.write_all(&bytes));
            }
            Ok(None) => break,
            Err((side, e)) => {
                written(out.flush());
                let (this, other) = match side {
                    audio::Side::Left => (&names[0], &names[1]),
                    audio::Side::Right => (&names[1], &names[0]),
                };
                match e {
                    audio::Error::Longer(n) => die(format!(
                        "the tracks aren't the same length: {this} has {n} samples more than {other}"
                    )),
                    audio::Error::NoEnd => die(format!("{this} {e}")),
                    e => die(format!("{this}: {e}")),
                }
            }
        }
    }
    written(out.flush());
}
