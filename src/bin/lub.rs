//! lub: play one channel of audio decoded by a CARDIAC (dub plays two: stereo). One CARDIAC runs the decoder,
//! `audio/decode.cq`, reading a card of the tape each time an INP asks for one. Its output is two values a sample,
//! the thousands and then the units; lub puts each pair back together and writes the sample to standard output as
//! soon as the CARDIAC outputs it. No other arithmetic is done here.
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
Usage: lub [OPTION]... [TAPE]
Decode one channel of audio on a CARDIAC, and output its samples as they are decoded.

TAPE is the channel as xtrack codes it, a card a line; with no TAPE, or TAPE -, it is read from standard input. The
samples go to standard output, raw: signed 16-bit little-endian, at the rate the audio was coded at. To play
them with ffplay (or write them to a file):

  lub song.left.tape | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout mono -

  -h, --help     display this help and exit
      --version  output version information and exit

Exit status is 0 if the tape was played to its end (or the player stopped reading), 1 if the tape can't be read
or isn't a tape the decoder can decode, 2 for a mistake on the command line.
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

    // the tape, a line at a time, read as the CARDIAC asks for cards
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
