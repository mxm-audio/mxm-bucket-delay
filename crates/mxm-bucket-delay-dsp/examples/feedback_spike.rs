//! B2: what the `Feedback` control has to be calibrated against, measured rather than derived.
//!
//! ```bash
//! cargo run -p mxm-bucket-delay-dsp --release --example feedback_spike
//! ```
//!
//! **This prints levels, not a verdict.** `mxm-folded-spring` learned that the hard way twice: one
//! measurement returned the ratio of two silences as `0.0 dB`, which looks exactly like a tone that
//! is not decaying, and another asked whether a tail was above a floor, which after a loud start
//! every setting is. Ask whether it is *decaying or holding*, over windows far enough apart to tell,
//! and print the numbers so a wrong one looks wrong.
//!
//! # There are two thresholds and calibrating to the wrong one is the trap
//!
//! A line sitting in silence has snapped to exact zero, so a small signal is cleared before the loop
//! can build on it: **from silence it needs more gain to start**. A line with anything in it needs
//! only enough to keep what is there. The control is mapped against the **running** figure, because
//! a delay somebody is playing through is never silent — that is the threshold a player meets.
//!
//! # Why this cannot be arithmetic here
//!
//! The compander is inside the loop. Its compressor pushes a *quiet* signal up, so a decaying tail
//! is boosted a little on every lap, and the gain at which the loop holds is lower than the takeoff
//! sum alone would say. That is a real property of a companded delay and it is the reason this
//! milestone is measurement rather than construction.

use mxm_bucket_delay_dsp::bbd::TAPS;
use mxm_bucket_delay_dsp::{Controls, Core, Line, Shape};

const FS: f32 = 48_000.0;

/// The reference tap setting the calibration is taken at: the last tap alone, which is the plain
/// single delay every echo circuit is, and what the Init preset opens.
fn reference_controls(feedback: f32) -> Controls {
    Controls {
        mix: 1.0,
        feedback,
        time_s: 0.25,
        taps: [0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        ..Controls::default()
    }
}

fn core_for(line: Line, feedback: f32) -> Core {
    let mut core = Core::new(FS);
    core.set_controls(reference_controls(feedback));
    core.set_shape(Shape {
        line,
        ..Shape::default()
    });
    core
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Run silence and return the RMS in two windows, seconds apart.
fn decay_windows(core: &mut Core, from_s: f32, to_s: f32, total_s: f32) -> (f32, f32) {
    let n = (total_s * FS) as usize;
    let mut early = Vec::new();
    let mut late = Vec::new();
    let win = (0.25 * FS) as usize;
    for i in 0..n {
        let (l, _) = core.process(0.0, 0.0, true);
        let t = i as f32 / FS;
        if t >= from_s && early.len() < win {
            early.push(l);
        }
        if t >= to_s && late.len() < win {
            late.push(l);
        }
    }
    (rms(&early), rms(&late))
}

/// Does an oscillation, once started, hold? Play a tone in, stop, and ask whether what is left is
/// decaying or holding — over four seconds, which is far enough apart to tell.
fn holds_running(line: Line, feedback: f32) -> bool {
    let mut core = core_for(line, feedback);
    for i in 0..(0.5 * FS) as usize {
        let x = 0.8 * (core::f32::consts::TAU * 220.0 * i as f32 / FS).sin();
        core.process(x, x, true);
    }
    let (early, late) = decay_windows(&mut core, 2.0, 5.0, 6.0);
    // Holding means it did not fall away over three seconds. A decaying loop loses far more than
    // this; a holding one gains or stays.
    late > early * 0.5 && late > 1e-4
}

/// Does it start from nothing? The line's own noise is the only seed, which is what the hardware
/// self-oscillates on too.
///
/// **Asked as "did it sing", not "is it growing".** The first version of this asked whether the
/// level was still rising between second 6 and second 11, which a loop that had already reached its
/// limit by second 4 is not — so every setting that worked read as a failure. That is the same
/// shape of mistake the spring's own notes warn about, made again here before it was caught.
fn starts_from_silence(line: Line, feedback: f32) -> bool {
    let mut core = core_for(line, feedback);
    let (_, late) = decay_windows(&mut core, 6.0, 11.0, 12.0);
    // The bare noise floor of the same line with no loop at all, for comparison.
    let mut quiet = core_for(line, 0.0);
    let (_, floor) = decay_windows(&mut quiet, 6.0, 11.0, 12.0);
    late > (floor * 20.0).max(1e-3)
}

/// The lowest gain at which `test` is true, to a thousandth.
fn threshold(line: Line, test: fn(Line, f32) -> bool) -> f32 {
    let (mut lo, mut hi) = (0.0f32, 3.0f32);
    if !test(line, hi) {
        return f32::NAN;
    }
    for _ in 0..12 {
        let mid = 0.5 * (lo + hi);
        if test(line, mid) { hi = mid } else { lo = mid }
    }
    hi
}

fn main() {
    println!("mxm-bucket-delay — B2, the feedback calibration");
    println!("{FS} Hz, 250 ms, the last tap alone at unity, Level 1.0\n");

    println!("| Line | Holds, running | Starts, from silence |");
    println!("|---|---|---|");
    let mut running = Vec::new();
    for line in Line::ALL {
        let holds = threshold(line, holds_running);
        let starts = threshold(line, starts_from_silence);
        running.push((line, holds));
        println!("| {:?} | {holds:.4} | {starts:.4} |", line);
    }

    println!("\nThe control curve maps travel to gain so that each line sings at the same place:");
    println!("  SING_AT = the *running* figure above, SINGS_AT_CONTROL = 0.9\n");
    for (line, holds) in &running {
        println!("  {:?}: SING_AT = {holds:.4}", line);
    }

    // What a listener meets: the level a settled oscillation lands at, per line, which is what says
    // whether the singing region is usable or a wall of noise.
    println!("\n| Line | Settled level at 1.1x its own threshold |");
    println!("|---|---|");
    for (line, holds) in &running {
        if holds.is_nan() {
            println!("| {line:?} | never sings |");
            continue;
        }
        let mut core = core_for(*line, holds * 1.1);
        for i in 0..(0.5 * FS) as usize {
            let x = 0.8 * (core::f32::consts::TAU * 220.0 * i as f32 / FS).sin();
            core.process(x, x, true);
        }
        let (_, late) = decay_windows(&mut core, 6.0, 9.0, 10.0);
        println!("| {line:?} | {late:.4} |");
    }

    // And the fact the whole calibration rests on: the two thresholds are far apart.
    println!("\nA line that has snapped to zero needs more gain to start than a running one needs");
    println!("to hold, which is why the control is mapped against the running figure.");
    let _ = TAPS;
}
