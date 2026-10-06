//! Where the Feedback control actually sings, at each tap setting.
//!
//! ```bash
//! cargo run -p mxm-bucket-delay-dsp --release --example feedback_travel
//! ```
//!
//! `feedback_spike` measures the *loop gain* at which a line holds. This asks the question a player
//! asks: **where on the knob does it happen** — and the answer depends on the tap faders, because
//! a fader is a term in the takeoff sum and therefore in the loop gain.
//!
//! The calibration was taken with the last tap alone at unity (the Init patch). Every other tap
//! setting moves the singing point, and this prints by how much.

use mxm_bucket_delay_dsp::bbd::{TAPS, feedback_gain};
use mxm_bucket_delay_dsp::{Controls, Core, Line, Shape};

const FS: f32 = 48_000.0;

/// The reference reverberation circuit's own resistor ladder, as `Reverberation` opens it.
const LADDER: [f32; TAPS] = [1.0, 0.909, 0.833, 0.769, 0.714, 0.667];
const ONE_TAP: [f32; TAPS] = [0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
const ALL_OPEN: [f32; TAPS] = [1.0; TAPS];
/// One tap at half. Distinguishes a law in the **sum** from a law in the **count**: the sum halves
/// while the count stays one.
const HALF_TAP: [f32; TAPS] = [0.0, 0.0, 0.0, 0.0, 0.0, 0.5];
/// Two taps at full: the count doubles while each fader stays where it was.
const TWO_TAPS: [f32; TAPS] = [0.0, 0.0, 0.0, 1.0, 0.0, 1.0];
/// Six taps at a third — the sum is two, the count is six.
const SIX_LOW: [f32; TAPS] = [1.0 / 3.0; TAPS];

/// The controls, with the knob at `travel`.
///
/// **No division here any more.** The normalisation now lives in the DSP (`applied_feedback`), so
/// this measures the *shipped* behaviour. While the law was being fitted this function applied a
/// candidate divisor itself and the DSP applied none; leaving it in after the law shipped measured
/// the division twice and reported that nothing sings at all, which is how a measurement lies.
/// **Nothing is compensated here.** This measures the shipped path and only that.
///
/// Twice while the mixer was being changed this function carried a divisor of its own, left over
/// from fitting an earlier law, and cancelled or doubled what the DSP was doing. Both times it
/// reported confidently and wrongly — once "nothing ever sings", once "everything sings at six per
/// cent". A measurement harness that compensates for the thing it is measuring is worse than none.
fn controls(line: Line, travel: f32, taps: [f32; TAPS]) -> Controls {
    Controls {
        mix: 1.0,
        feedback: feedback_gain(line, travel),
        time_s: 0.25,
        taps,
        ..Controls::default()
    }
}

/// Does a tail started by a tone still hold three seconds after the input stops?
fn holds(line: Line, travel: f32, taps: [f32; TAPS]) -> bool {
    let mut core = Core::new(FS);
    core.set_controls(controls(line, travel, taps));
    core.set_shape(Shape {
        line,
        ..Shape::default()
    });
    for i in 0..(0.5 * FS) as usize {
        let x = 0.8 * (core::f32::consts::TAU * 220.0 * i as f32 / FS).sin();
        core.process(x, x, true);
    }
    let rms = |core: &mut Core, seconds: f32| {
        let n = (0.25 * FS) as usize;
        let skip = (seconds * FS) as usize;
        for _ in 0..skip {
            core.process(0.0, 0.0, true);
        }
        let mut sum = 0.0f32;
        for _ in 0..n {
            let (l, _) = core.process(0.0, 0.0, true);
            sum += l * l;
        }
        (sum / n as f32).sqrt()
    };
    let early = rms(&mut core, 1.0);
    let late = rms(&mut core, 2.5);
    late > early * 0.5 && late > 1e-4
}

/// The lowest travel at which it holds, to a hundredth of the knob.
fn sings_at_travel(line: Line, taps: [f32; TAPS]) -> Option<f32> {
    (0..=100)
        .map(|n| n as f32 / 100.0)
        .find(|travel| holds(line, *travel, taps))
}

fn main() {
    println!(
        "mxm-bucket-delay - where on the Feedback knob the line starts to sing
"
    );
    println!(
        "Calibrated against the last tap alone, so every column should read about 90 %.
"
    );
    println!(
        "| Line | one tap | ladder | six open | one tap at half | two taps | six at a third |"
    );
    println!("|---|---|---|---|---|---|---|");
    for line in Line::ALL {
        let cell = |taps| match sings_at_travel(line, taps) {
            Some(travel) => format!("{:.0} %", travel * 100.0),
            None => "never".to_owned(),
        };
        println!(
            "| {line:?} | {} | {} | {} | {} | {} | {} |",
            cell(ONE_TAP),
            cell(LADDER),
            cell(ALL_OPEN),
            cell(HALF_TAP),
            cell(TWO_TAPS),
            cell(SIX_LOW)
        );
    }
}
