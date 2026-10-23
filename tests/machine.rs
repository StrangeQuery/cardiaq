//! The machine a debugger holds (`cardiac::Machine`): breakpoints, steps, blocking input, tapes fed, and the same
//! state as a plain run at every step.
use heartburn::cardiac::{self, Event, Machine, Stop};
use heartburn::listing::{self, Listing};

fn deck(start: usize, cells: &[(usize, i32)]) -> Vec<i32> {
    listing::deck(start, cells).unwrap()
}

fn listed(text: &str) -> Vec<i32> {
    let Listing::Memory(cells) = listing::parse(text).unwrap().listing else {
        panic!("a memory listing")
    };
    deck(3, &cells)
}

/// Counts 1, 2, 3 into cell 20, punching each: 10 CLA 20, 11 ADD 00, 12 STO 20, 13 OUT 20, 14 SUB 21, 15 TAC 10,
/// 16 HRS 00; 21 holds 3.
fn counts() -> Vec<i32> {
    deck(
        10,
        &[
            (10, 120),
            (11, 200),
            (12, 620),
            (13, 520),
            (14, 721),
            (15, 310),
            (16, 900),
            (21, 3),
        ],
    )
}

#[test]
fn a_breakpoint_stops_before_its_cell_and_clearing_it_runs_on() {
    let mut m = Machine::with_tapes(&counts(), &[]);
    let mut out = vec![];
    m.set_break(13);
    assert_eq!(m.run(1000, &mut out), Event::Break);
    assert_eq!((m.pc(), m.cell(20), out.clone()), (13, 1, vec![]));
    // run on from it: its instruction runs, and the loop comes round to it again
    assert_eq!(m.run(1000, &mut out), Event::Break);
    assert_eq!((m.pc(), m.cell(20), out.clone()), (13, 2, vec![1]));
    m.clear_break(13);
    assert_eq!(m.run(1000, &mut out), Event::Halt);
    assert_eq!((out, m.pc()), (vec![1, 2, 3], 0));
}

#[test]
fn a_breakpoint_on_a_cell_the_program_writes_stays() {
    // 10 CLA 30, 11 STO 12, 12 HRS 00 (until 11 makes it OUT 31), 13 HRS 00; 30 holds 531 (OUT 31), 31 holds 42
    let d = deck(10, &[(10, 130), (11, 612), (12, 900), (13, 900), (30, 531), (31, 42)]);
    let mut m = Machine::with_tapes(&d, &[]);
    let mut out = vec![];
    m.set_break(12);
    assert_eq!(m.run(1000, &mut out), Event::Break);
    assert_eq!((m.pc(), m.cell(12), out.clone()), (12, 531, vec![]));
    assert_eq!(m.run(1000, &mut out), Event::Halt);
    assert_eq!(out, [42]);
    // and a cell written by hand keeps its breakpoint too
    m.set_cell(12, 900);
    m.set_pc(11);
    assert_eq!(m.run(1000, &mut out), Event::Break);
    assert_eq!((m.pc(), m.cell(12)), (12, 531));
}

#[test]
fn a_step_count_stops_exactly() {
    let mut m = Machine::with_tapes(&counts(), &[]);
    let mut out = vec![];
    assert_eq!(m.run(7, &mut out), Event::Budget);
    assert_eq!(m.steps(), 7);
    for n in 8..=12 {
        assert_eq!(m.step(&mut out), Event::Budget);
        assert_eq!(m.steps(), n);
    }
    assert_eq!(m.run(0, &mut out), Event::Budget);
    assert_eq!(m.steps(), 12);
}

#[test]
fn an_inp_with_no_card_waits_for_one() {
    // 10 INP 20, 11 OUT 20, 12 JMP 10
    let d = deck(10, &[(10, 20), (11, 520), (12, 810)]);
    let mut m = Machine::with_tapes(&d, &[]);
    let mut out = vec![];
    assert_eq!(m.run(1000, &mut out), Event::Input);
    let (pc, steps) = (m.pc(), m.steps());
    assert_eq!((pc, m.next_card()), (10, None));
    // waiting doesn't run anything
    assert_eq!(m.run(1000, &mut out), Event::Input);
    assert_eq!((m.pc(), m.steps()), (pc, steps));
    m.push(&[7, -8]);
    assert_eq!(m.next_card(), Some(7));
    assert_eq!(m.run(1000, &mut out), Event::Input);
    assert_eq!((out, m.steps()), (vec![7, -8], steps + 6));
}

#[test]
fn hrs_n_feeds_tape_n() {
    // phase 1: 03 INP 10, 04 OUT 10, 05 HRS 02; phase 2: 03 INP 11, 04 OUT 11, 05 HRS 00
    let one = deck(3, &[(3, 10), (4, 510), (5, 902)]);
    let two = deck(3, &[(3, 11), (4, 511), (5, 900)]);
    let mut first = one.clone();
    first.extend([5, 6]);
    let mut m = Machine::with_tapes(&first, std::slice::from_ref(&two));
    let mut out = vec![];
    assert_eq!(m.run(1000, &mut out), Event::Tape(2));
    assert_eq!(
        (m.pc(), m.phase(), m.next_card(), out.clone()),
        (0, 2, Some(2), vec![5])
    );
    assert_eq!(m.run(1000, &mut out), Event::Halt);
    assert_eq!(out, [5, 6]);
    // with no tape 2, HRS 02 halts with the bug on 02
    let mut m = Machine::with_tapes(&first, &[]);
    assert_eq!(m.run(1000, &mut vec![]), Event::Halt);
    assert_eq!(m.pc(), 2);
}

/// At any step, the machine a debugger holds is where a plain run stopped at that step: memory, accumulator, cards
/// punched; and it ends the same, after the same number of instructions.
#[test]
fn state_matches_a_plain_run_at_every_step() {
    let one = listed(include_str!("../examples/sortem/sortem-1.lst"));
    let two = listed(include_str!("../examples/sortem/sortem-2.lst"));
    let input: Vec<i32> = (0..35).map(|i| (i * 37 % 101) - 50).collect();
    let later = vec![two];
    let mut first = one.clone();
    first.extend(&input);
    let total =
        cardiac::run_reading::<false>(&one, &input, &later, &mut std::iter::empty(), &mut vec![], u64::MAX).steps;
    let mut m = Machine::with_tapes(&first, &later);
    let mut punched = vec![];
    let mut halted = false;
    for n in (0..total).step_by(613).chain([total - 1, total]) {
        // run the machine on to step n, through the stop for tape 2
        while m.steps() < n {
            match m.run(n - m.steps(), &mut punched) {
                Event::Budget | Event::Tape(2) => {}
                Event::Halt if m.steps() == total => halted = true,
                e => panic!("{e:?} at {}", m.steps()),
            }
        }
        let mut plain = vec![];
        let end = cardiac::run_reading::<false>(&one, &input, &later, &mut std::iter::empty(), &mut plain, n);
        assert_eq!(
            (m.memory(), m.acc(), m.steps()),
            (&end.memory, end.acc, end.steps),
            "at step {n}"
        );
        assert_eq!(punched, plain, "at step {n}");
    }
    let end = cardiac::run_reading::<false>(&one, &input, &later, &mut std::iter::empty(), &mut vec![], u64::MAX);
    assert_eq!((end.stop, halted), (Stop::Halt, true));
}
