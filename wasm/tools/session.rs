//! The native side of the wasm tests: runs a session script (test/compare.py writes them) on a `cardiac::Machine`
//! and prints what happened, line for line as test/session.js prints it for heartburn.wasm.
//!
//! Script lines: `tape CARDS` (the next tape), `cells A V ...` (direct load), `pc N`, `acc N`, `push CARDS`,
//! `input CARDS` (one put in each time an INP waits), `break N`, `step N` (N single steps now), `budget N`,
//! `runs N`. After the script: up to `runs` runs of `budget` instructions, until it halts or waits with no input.
use heartburn::cardiac::{Event, Machine};

fn code(e: Event) -> i32 {
    match e {
        Event::Budget => 0,
        Event::Halt => 1,
        Event::Break => 2,
        Event::Input => 3,
        Event::Tape(n) => 100 + n as i32,
    }
}

fn join<T: ToString>(v: impl IntoIterator<Item = T>) -> String {
    v.into_iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ")
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: session SCRIPT");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut m = Machine::new();
    let (mut tapes, mut input, mut punched, mut events) = (vec![], vec![], vec![], vec![]);
    let (mut budget, mut runs) = (1_000_000u64, 100usize);
    for line in text.lines() {
        let mut w = line.split_whitespace();
        let Some(word) = w.next() else { continue };
        let n: Vec<i64> = w.map(|x| x.parse().unwrap()).collect();
        let cards = || n.iter().map(|&x| x as i32).collect::<Vec<i32>>();
        match word {
            "tape" => {
                tapes.push(cards());
                m.set_tapes(tapes.clone());
            }
            "cells" => m.load_cells(&n.chunks(2).map(|p| (p[0] as usize, p[1] as i32)).collect::<Vec<_>>()),
            "pc" => m.set_pc(n[0] as usize),
            "acc" => m.set_acc(n[0] as i32),
            "push" => m.push(&cards()),
            "input" => input.extend(cards()),
            "break" => m.set_break(n[0] as usize),
            "step" => {
                for _ in 0..n[0] {
                    events.push(format!("s{}@{}", code(m.step(&mut punched)), m.pc()));
                }
            }
            "budget" => budget = n[0] as u64,
            "runs" => runs = n[0] as usize,
            _ => panic!("not a script line: {line}"),
        }
    }
    let mut input = input.into_iter();
    for _ in 0..runs {
        let e = m.run(budget, &mut punched);
        events.push(format!("{}@{}", code(e), m.pc()));
        match e {
            Event::Halt => break,
            Event::Input => match input.next() {
                Some(c) => m.push(&[c]),
                None => break,
            },
            _ => {}
        }
    }
    println!("events: {}", events.join(" "));
    println!("punched: {}", join(&punched));
    println!("steps: {}", m.steps());
    println!("acc: {}", m.acc());
    println!("pc: {}", m.pc());
    println!("phase: {}", m.phase());
    println!("memory: {}", join(m.memory()));
    println!("owners: {}", join(m.owners()));
    println!("written: {}", join(m.written_at()));
    println!("trail: {}", join(m.trail()));
    println!("reader: {}", join(m.cards()));
    println!("ran: {}", join(m.ran().iter().map(|&b| b as u8)));
    println!("used: {}", join(m.used().iter().map(|&b| b as u8)));
    let (cell, word, acc) = m.last();
    println!("last: {cell} {word} {acc}");
}
