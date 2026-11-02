//! dub: play two channels of audio, left and right, each decoded by a CARDIAC of its own (lub plays one). Both
//! CARDIACs run the decoder, `audio/decode.cq`, on one thread, taking turns a block at a time: each turn, the left
//! one is given its tape's next block of cards and runs until it asks for more, then the right one. Each outputs
//! two values a sample, the thousands and then the units; dub puts each pair back together and writes the samples
//! to standard output, left then right, as a stereo stream. No other arithmetic is done here.
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, IsTerminal, Write};

use heartburn::audio;
use heartburn::cardiac::{Console, Event, Machine};
use heartburn::tape;

#[path = "../getopt.rs"]
mod getopt;

const PROG: &str = "dub";

fn usage() -> &'static str {
    "\
Usage: dub [OPTION]... LEFT RIGHT
Decode two channels of audio, each on a CARDIAC, and output them as one stereo stream as they are decoded.

LEFT and RIGHT are the channels' tapes as pace codes them, a card a line; one of them may be -, standard input.
The samples go to standard output, raw: signed 16-bit little-endian, left then right, at the rate the audio was
coded at. To play them with ffplay (or write them to a file):

  dub song.left.tape song.right.tape | ffplay -nodisp -autoexit -f s16le -ar 48000 -ch_layout stereo -

  -h, --help     display this help and exit
      --version  output version information and exit

Exit status is 0 if the tapes were played to their end (or the player stopped reading), 1 if a tape can't be
read, isn't a tape the decoder can decode, or the two aren't the same length, 2 for a mistake on the command
line.
"
}

fn die(why: impl std::fmt::Display) -> ! {
    getopt::die(PROG, why)
}

/// A CARDIAC's output, combined: a sample for each two values, kept until the other channel's is there too.
struct Samples {
    name: String,
    thousands: Option<i32>,
    decoded: VecDeque<i16>,
}

impl Console for Samples {
    fn output(&mut self, value: i32) {
        let Some(thousands) = self.thousands.take() else {
            self.thousands = Some(value);
            return;
        };
        let Some(sample) = audio::sample(thousands, value) else {
            die(format!(
                "{}: the decoder output {thousands} and {value}, which aren't a 16-bit sample",
                self.name
            ))
        };
        self.decoded.push_back(sample);
    }
}

/// A channel: its CARDIAC, the cards of its tape still to go in, and the samples decoded.
struct Channel {
    cardiac: Machine,
    cards: Box<dyn Iterator<Item = i32>>,
    samples: Samples,
    done: bool,
}

impl Channel {
    fn new(name: &str) -> Channel {
        let shown = if name == "-" {
            "standard input".to_string()
        } else {
            name.to_string()
        };
        let reader: Box<dyn BufRead> = if name == "-" {
            Box::new(io::stdin().lock())
        } else {
            Box::new(BufReader::new(
                File::open(name).unwrap_or_else(|e| die(format!("{name}: {}", getopt::os_error(&e)))),
            ))
        };
        let at = shown.clone();
        let cards = reader.lines().enumerate().flat_map(move |(n, line)| {
            let line = line.unwrap_or_else(|e| die(format!("{at}: {}", getopt::os_error(&e))));
            tape::values(&line).unwrap_or_else(|e| die(format!("{at}: line {}: {e}", n + 1)))
        });
        Channel {
            cardiac: Machine::with_tapes(&audio::decoder().phases, &[]),
            cards: Box::new(cards),
            samples: Samples {
                name: shown,
                thousands: None,
                decoded: VecDeque::new(),
            },
            done: false,
        }
    }

    /// The channel's turn: its tape's next block of cards into the reader, and the CARDIAC run until it asks for
    /// more or halts at the end card.
    fn turn(&mut self) {
        if self.done {
            return;
        }
        let block: Vec<i32> = self.cards.by_ref().take(audio::BLOCK + 2).collect();
        self.cardiac.push(&block);
        match self.cardiac.run(u64::MAX, &mut self.samples) {
            Event::Halt if self.samples.thousands.is_none() => self.done = true,
            Event::Halt => die(format!(
                "{}: the decoder stopped halfway through a sample",
                self.samples.name
            )),
            Event::Input if block.is_empty() => die(format!("{} ended without its end card (-1)", self.samples.name)),
            Event::Input => {}
            other => unreachable!("the decoder has no breakpoints and no phases: {other:?}"),
        }
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
        [l, r] if l == "-" && r == "-" => getopt::usage_error(PROG, "only one tape can be standard input"),
        [l, r] => (l.as_str(), r.as_str()),
        [] | [_] => getopt::usage_error(PROG, "two tapes: LEFT and RIGHT"),
        [_, _, extra, ..] => getopt::usage_error(PROG, format!("extra operand '{extra}'")),
    };
    if io::stdout().is_terminal() {
        getopt::usage_error(
            PROG,
            "standard output is a terminal: pipe the samples to a player, or to a file",
        )
    }
    let (mut left, mut right) = (Channel::new(left), Channel::new(right));
    let mut out = BufWriter::new(io::stdout().lock());
    let written = |r: io::Result<()>| match r {
        Ok(()) => {}
        // the player stopped reading: that's the end, not an error
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => std::process::exit(0),
        Err(e) => die(format!("standard output: {}", getopt::os_error(&e))),
    };
    while !(left.done && right.done) {
        left.turn();
        right.turn();
        // a stereo stream is the channels' samples taken in turn, left then right
        while !left.samples.decoded.is_empty() && !right.samples.decoded.is_empty() {
            let l = left.samples.decoded.pop_front().unwrap();
            let r = right.samples.decoded.pop_front().unwrap();
            written(
                out.write_all(&l.to_le_bytes())
                    .and_then(|_| out.write_all(&r.to_le_bytes())),
            );
        }
    }
    written(out.flush());
    let (l, r) = (left.samples.decoded.len(), right.samples.decoded.len());
    if l != r {
        die(format!(
            "the tapes aren't the same length: {} has {} samples more than {}",
            if l > r { &left.samples.name } else { &right.samples.name },
            l.max(r),
            if l > r { &right.samples.name } else { &left.samples.name },
        ))
    }
}
