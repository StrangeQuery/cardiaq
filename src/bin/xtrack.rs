//! xtrack encodes one channel of audio into a track for the CARDIAC decoder, `audio/decode.cq`, which lub and dub
//! run. The encoding itself is `heartburn::audio::encode`. This program reads the samples and writes the cards.
use std::io::{IsTerminal, Read, Write};

use heartburn::audio;

#[path = "../getopt.rs"]
mod getopt;

const PROG: &str = "xtrack";

fn usage() -> &'static str {
    "\
Usage: xtrack [OPTION]... [FILE]
Encode one channel of audio into a track for a CARDIAC to decode. lub and dub play tracks.

FILE holds raw samples for one channel, as signed 16-bit little-endian numbers. Any sample rate works, for
example 48000 samples per second. With no FILE, or when FILE is -, the samples are read from standard input.

The track goes to standard output, one card per line. Each block of 64 samples starts with two cards, K and M,
which give the block's step size. Then come 64 cards, one per sample. The last card is -1, which marks the end.

This makes the samples for the left channel of song.wav at 48 kHz with ffmpeg, and encodes them:

  ffmpeg -i song.wav -af 'pan=mono|c0=c0' -ar 48000 -f s16le - | xtrack > song.left.track

  -h, --help     display this help and exit
      --version  output version information and exit

The exit status is 0 if the track was written, 1 if the samples can't be read or the track can't be written,
and 2 if there is a mistake on the command line.
"
}

fn die(why: impl std::fmt::Display) -> ! {
    getopt::die(PROG, why)
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
    let mut bytes = vec![];
    if name == "-" {
        if std::io::stdin().is_terminal() {
            getopt::usage_error(
                PROG,
                "standard input is a terminal: give it raw samples, or name a FILE",
            )
        }
        std::io::stdin()
            .read_to_end(&mut bytes)
            .unwrap_or_else(|e| die(format!("standard input: {}", getopt::os_error(&e))));
    } else {
        bytes = std::fs::read(name).unwrap_or_else(|e| die(format!("{name}: {}", getopt::os_error(&e))));
    }
    let shown = if name == "-" { "standard input" } else { name };
    if bytes.len() % 2 != 0 {
        die(format!("{shown}: an odd number of bytes: not 16-bit samples"))
    }
    let samples: Vec<i16> = bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    let (cards, _) = audio::encode(&samples);
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for c in cards {
        writeln!(out, "{c}").unwrap_or_else(|e| die(format!("standard output: {}", getopt::os_error(&e))));
    }
    out.flush()
        .unwrap_or_else(|e| die(format!("standard output: {}", getopt::os_error(&e))));
}
