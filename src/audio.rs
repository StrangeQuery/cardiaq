//! Audio that a CARDIAC decodes, using a form of ADPCM designed around CARDIAC's decimal arithmetic. The encoder
//! runs here, on the host, ahead of time. The decoder is a CARDIAC program, `audio/decode.cq`, and the CARDIAC
//! calculates every sample it outputs.
//!
//! A channel is encoded as a track of cards, in blocks of 64 samples. Each block starts with two cards, K (0-3) and
//! M (1-9), and the block's step size is M * 10^K. Then comes one card per sample, a code C from -500 to 499. Each
//! sample is a prediction plus C times the step size. The prediction is a straight line through the previous two
//! samples: twice the previous sample, minus the one before it. A K below zero marks the end of the track.
//!
//! The decoder outputs each sample as two values, first the thousands and then the units (0-999), because a single
//! value from -999 to 999 doesn't have enough levels for good sound. `sample` joins a pair back into one sample.
use crate::assemble;
use crate::tape::Tape;

/// The samples in a block.
pub const BLOCK: usize = 64;

/// The decoder's listing.
pub const DECODER: &str = include_str!("../audio/decode.cq");

/// The decoder, assembled as carc assembles it.
pub fn decoder() -> Tape {
    let asm = assemble::assemble(DECODER);
    assert!(
        asm.diagnostics.is_empty(),
        "audio/decode.cq doesn't assemble cleanly: {:?}",
        asm.diagnostics
    );
    Tape::one(assemble::deck(&asm, asm.start.expect("audio/decode.cq has cells")))
}

/// A sample from the decoder's two values for it, the thousands and the units; none if they aren't a 16-bit sample.
pub fn sample(thousands: i32, units: i32) -> Option<i16> {
    if !(0..=999).contains(&units) {
        return None;
    }
    i16::try_from(thousands * 1000 + units).ok()
}

/// `n / d` to the nearest whole number, halves to the even one, for d above 0.
fn nearest(n: i64, d: i64) -> i64 {
    let (q, r) = (n.div_euclid(d), n.rem_euclid(d));
    match (2 * r).cmp(&d) {
        std::cmp::Ordering::Less => q,
        std::cmp::Ordering::Greater => q + 1,
        std::cmp::Ordering::Equal => q + (q & 1),
    }
}

/// A block coded: each code, with the sample the decoder makes of it.
type Coded = Vec<(i32, i16)>;

/// One block coded with one step, from the decoder's last two samples: its squared error, and each code with the
/// sample the decoder makes of it; none if the step can't keep every sample within 16 bits. Each code is chosen
/// against the decoder's own prediction (closed loop).
fn code_block(xs: &[i16], mut s1: i64, mut s2: i64, step: i64) -> Option<(i64, Coded)> {
    let mut err = 0;
    let mut out = Vec::with_capacity(xs.len());
    for &x in xs {
        let e = 2 * s1 - s2;
        // the codes that keep the sample within 16 bits
        let lo = (-(32768 + e).div_euclid(step)).max(-500);
        let hi = (32767 - e).div_euclid(step).min(499);
        if lo > hi {
            return None;
        }
        let c = nearest(x as i64 - e, step).clamp(lo, hi);
        let s = e + c * step;
        err += (x as i64 - s) * (x as i64 - s);
        out.push((c as i32, s as i16));
        (s2, s1) = (s1, s);
    }
    Some((err, out))
}

/// A channel's track (its last card the end, -1), and the samples the decoder will output from it. Each block takes
/// the step with the least squared error, the first such in the order K 0-3, M 1-9, of those that keep it within
/// 16 bits; the last block is filled out with its last sample.
pub fn encode(samples: &[i16]) -> (Vec<i32>, Vec<i16>) {
    let (mut cards, mut decoded) = (vec![], vec![]);
    let (mut s1, mut s2) = (0, 0);
    for chunk in samples.chunks(BLOCK) {
        let mut block = chunk.to_vec();
        block.resize(BLOCK, chunk[chunk.len() - 1]);
        // the best step so far: its error, K, M, and the block coded with it
        let mut best: Option<(i64, i32, i32, Coded)> = None;
        for k in 0..4 {
            for m in 1..10 {
                let Some((err, out)) = code_block(&block, s1, s2, m as i64 * 10i64.pow(k as u32)) else {
                    continue;
                };
                if best.as_ref().is_none_or(|b| err < b.0) {
                    best = Some((err, k, m, out));
                }
            }
        }
        let (_, k, m, out) = best.expect("9 * 10^3 reaches 16 bits from any prediction");
        cards.extend([k, m]);
        cards.extend(out.iter().map(|&(c, _)| c));
        decoded.extend(out.iter().map(|&(_, s)| s));
        (s1, s2) = (out[BLOCK - 1].1 as i64, out[BLOCK - 2].1 as i64);
    }
    cards.push(-1);
    (cards, decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cardiac::Stop;

    /// Music-like test samples: two tones, a click, noise, and stretches at both ends of 16 bits.
    fn test_samples(n: usize) -> Vec<i16> {
        let mut seed = 12345u32;
        (0..n)
            .map(|i| {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let noise = ((seed >> 16) as f64 / 65536.0 - 0.5) * 600.0;
                let t = i as f64 / 48000.0;
                let v = 9000.0 * (t * 440.0 * std::f64::consts::TAU).sin()
                    + 4000.0 * (t * 3520.0 * std::f64::consts::TAU).sin()
                    + noise
                    + if i % 2000 == 0 { 20000.0 } else { 0.0 };
                match i {
                    3000..=3300 => 32767,
                    3301..=3600 => -32768,
                    _ => v.clamp(-32768.0, 32767.0) as i16,
                }
            })
            .collect()
    }

    /// The decoder on CARDIAC outputs exactly the samples the encoder says it will, and halts at the end card.
    #[test]
    fn the_cardiac_decodes_what_the_encoder_codes() {
        let xs = test_samples(5000);
        let (cards, expected) = encode(&xs);
        assert_eq!(cards.len(), 5000usize.div_ceil(BLOCK) * (BLOCK + 2) + 1);
        let (out, stop, _) = decoder().run(&cards, u64::MAX).unwrap();
        assert_eq!(stop, Stop::Halt);
        let got: Vec<i16> = out.chunks(2).map(|p| sample(p[0], p[1]).unwrap()).collect();
        assert_eq!(got, expected);
        // and it's close: the error is a small part of the signal
        let err: f64 = xs
            .iter()
            .zip(&expected)
            .map(|(&a, &b)| (a as f64 - b as f64).powi(2))
            .sum();
        let sig: f64 = xs.iter().map(|&a| (a as f64).powi(2)).sum();
        assert!(10.0 * (sig / err).log10() > 50.0, "{} dB", 10.0 * (sig / err).log10());
    }

    #[test]
    fn a_pair_is_a_sample_only_in_16_bits() {
        assert_eq!(sample(-33, 232), Some(-32768));
        assert_eq!(sample(32, 767), Some(32767));
        assert_eq!(sample(32, 768), None);
        assert_eq!(sample(1, 1000), None);
        assert_eq!(sample(-1, -5), None);
    }

    #[test]
    fn nothing_in_is_just_the_end() {
        let (cards, decoded) = encode(&[]);
        assert_eq!((cards, decoded), (vec![-1], vec![]));
    }
}
