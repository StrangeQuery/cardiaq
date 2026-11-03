//! lub plays one channel of audio decoded by a CARDIAC. (dub plays two channels, in stereo.) A single CARDIAC runs
//! the decoder, `audio/decode.cq`, and lub gives it the next card of the track each time an INP asks for one. The
//! decoder outputs two values for every sample, first the thousands and then the units. lub joins each pair back
//! into one sample and writes it to standard output as soon as the CARDIAC outputs it. lub does no other
//! arithmetic.
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, IsTerminal, Write};

use heartburn::audio;
use heartburn::cardiac::{Console, Stop};
use heartburn::tape;

#[path = "../getopt.rs"]
mod getopt;

const PROG: &str = "lub";

fn usage() -> &'static str {
    "\
Usage: lub [OPTION]... [TRACK]
Decode one channel of audio on a CARDIAC, and output each sample as soon as it is decoded.

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

/// The decoder's output, combined: a sample for each two values, written as it is made.
struct Samples<W: Write> {
    out: W,
    thousands: Option<i32>,
}

impl<W: Write> Samples<W> {
    fn written(&mut self, r: io::Result<()>) {
        match r {
            Ok(()) => {}
            // the player stopped reading: that's the end, not an error
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => std::process::exit(0),
            Err(e) => die(format!("standard output: {}", getopt::os_error(&e))),
        }
    }
}

impl<W: Write> Console for Samples<W> {
    fn output(&mut self, value: i32) {
        let Some(thousands) = self.thousands.take() else {
            self.thousands = Some(value);
            return;
        };
        let Some(sample) = audio::sample(thousands, value) else {
            die(format!(
                "the decoder output {thousands} and {value}, which aren't a 16-bit sample"
            ))
        };
        let r = self.out.write_all(&sample.to_le_bytes());
        self.written(r);
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
    let mut cards = reader.lines().enumerate().flat_map(|(n, line)| {
        let line = line.unwrap_or_else(|e| die(format!("{shown}: {}", getopt::os_error(&e))));
        tape::values(&line).unwrap_or_else(|e| die(format!("{shown}: line {}: {e}", n + 1)))
    });
    let mut samples = Samples {
        out: BufWriter::new(io::stdout().lock()),
        thousands: None,
    };
    let (stop, _) = audio::decoder()
        .run_into(&[], &mut cards, u64::MAX, &mut samples)
        .unwrap_or_else(|e| die(e));
    let r = samples.out.flush();
    samples.written(r);
    match stop {
        Stop::Halt if samples.thousands.is_none() => {}
        Stop::Halt => die("the decoder stopped halfway through a sample"),
        Stop::OutOfCards => die(format!("{shown} ended without its end card (-1)")),
        Stop::Limit => unreachable!("lub runs without a limit"),
    }
}
