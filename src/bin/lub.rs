//! lub plays one channel of audio decoded by a CARDIAC. (dub plays two channels, in stereo.) The decoding is
//! `heartburn::audio::Channel`: a CARDIAC running the decoder, `audio/decode.cq`, which turns each block of the
//! track's cards into a block of samples. lub only reads the track and writes the samples to standard output.
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, IsTerminal, Write};

use heartburn::audio;
use heartburn::tape;

#[path = "../getopt.rs"]
mod getopt;

const PROG: &str = "lub";

fn usage() -> &'static str {
    "\
Usage: lub [OPTION]... [TRACK]
Decode one channel of audio on a CARDIAC, and output the samples as they are decoded.

TRACK is a track made by xtrack, with one card per line. With no TRACK, or when TRACK is -, the track is read
from standard input. The samples go to standard output as raw signed 16-bit little-endian numbers, at the
sample rate the audio was encoded at. To play them with ffplay:

  lub song.left.track | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout mono -

  -h, --help     display this help and exit
      --version  output version information and exit

The exit status is 0 if the track was played to the end, or if the player stopped reading. It is 1 if the
track can't be read or the decoder can't decode it, and 2 if there is a mistake on the command line.
"
}

fn die(why: impl std::fmt::Display) -> ! {
    getopt::die(PROG, why)
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
    let name = match &operands[..] {
        [] => "-",
        [f] => f.as_str(),
        [_, extra, ..] => getopt::usage_error(PROG, format!("extra operand '{extra}'")),
    };
    if io::stdout().is_terminal() {
        getopt::usage_error(
            PROG,
            "standard output is a terminal: pipe the samples to a player, or to a file",
        )
    }
    let reader: Box<dyn BufRead> = if name == "-" {
        Box::new(io::stdin().lock())
    } else {
        Box::new(BufReader::new(
            File::open(name).unwrap_or_else(|e| die(format!("{name}: {}", getopt::os_error(&e)))),
        ))
    };
    let shown = if name == "-" { "standard input" } else { name };

    // the track, a line at a time, read as the CARDIAC asks for cards
    let cards = reader.lines().enumerate().flat_map(|(n, line)| {
        let line = line.unwrap_or_else(|e| die(format!("{shown}: {}", getopt::os_error(&e))));
        tape::values(&line).unwrap_or_else(|e| die(format!("{shown}: line {}: {e}", n + 1)))
    });
    let mut channel = audio::Channel::new(cards);
    let mut out = BufWriter::new(io::stdout().lock());
    loop {
        match channel.next_block() {
            Ok(Some(block)) => {
                let bytes: Vec<u8> = block.iter().flat_map(|s| s.to_le_bytes()).collect();
                written(out.write_all(&bytes));
            }
            Ok(None) => break,
            Err(e @ audio::Error::NoEnd) => die(format!("{shown} {e}")),
            Err(e) => die(e),
        }
    }
    written(out.flush());
}
