//! What each factory preset actually does, measured through the shipped plugin.
//!
//! ```bash
//! cargo run -p mxm-bucket-delay --release --example bucket_delay_preset_audit
//! ```
//!
//! **Written because the six presets were authored against two laws that have since changed**, and
//! a preset is the one thing in the product that no test covers: it is a set of numbers whose only
//! contract is that it sounds like its name.
//!
//! - **`Feedback` was re-fitted twice** (see the DSP contract). Before the fix the loop gain was the
//!   control times the takeoff sum, so a patch with two taps open sang at 55 % of the travel and one
//!   with six sang at 16 %. Every preset's `Feedback` was chosen against *that* knob, and
//!   `Reverberation` — the six-tap patch the whole product argues for — was left doing almost
//!   nothing: one pass through the line, no regeneration, a 0.7 s tail.
//! - **`Level` became `Mix`**, a crossfade. The six values were rescaled by `v / (1 + v)`, the
//!   arithmetic that holds each patch's own wet-to-dry ratio, which is honest but is nobody's
//!   *choice*. Under a crossfade there is a real decision to make per patch.
//!
//! # Three stimuli, because one cannot be fair to fifty presets
//!
//! This harness got the answer wrong **three times** before it got it right, and each mistake looked
//! exactly like a preset that needed fixing:
//!
//! 1. A **one-second note** hid a 300 ms delay's whole wash inside itself, and `Reverberation` read
//!    as inaudible.
//! 2. A **150 ms pluck** barely fills `Reverse swell`'s three-quarter-second window, so that preset
//!    read as dead at *every* feedback setting. Its `Feedback` was raised on that reading and put
//!    back on the next one.
//! 3. A **pure sine** made `Chamber`, `Wide bloom` and `Gallop` read as clipping at 1.2 to 1.5.
//!    They were trimmed for it, twice, before the arithmetic said why: a sine is the
//!    maximum-correlation input, the tap mixer normalises `√Σg²` for *decorrelated* returns by
//!    design, and three taps at 0.6/0.8/1.0 therefore sum to 1.70× one tap on a tone and to one on
//!    program material. Their character was restored and only the level trim kept.
//!
//! So all three are reported: a **pluck**, which is how an echo is judged; a **held note** with
//! twelve harmonics, which is how a wash is judged and what the levels are set by; and a **pure
//! tone**, whose peak is printed beside the others as the headroom a correlated input eats. Read
//! the column that matches how the preset is played.
//!
//! The numbers are the evidence; the ear is still the judge. Nothing here asserts, because there is
//! no correct answer to assert against — this reports, and a person chooses.
//! `tests/the_factory_bank.rs` is the floor under it: no preset may clip on program material, and
//! none may be inaudible.

use mxm_bucket_delay::params::MxmBucketDelayParams;
use mxm_bucket_delay::{MxmBucketDelay, preset};
use mxm_bucket_delay_dsp::bbd::feedback_gain;
use nice_plug::params::InternalParamMut;
use nice_plug::prelude::*;

const FS: f32 = 48_000.0;

/// How long the pluck lasts.
const NOTE_S: f32 = 0.15;
/// How long the held note lasts, for the presets a pluck cannot fill.
const HELD_S: f32 = 1.5;
/// The silence after the note that the output is measured over.
const WINDOW_S: f32 = 1.5;

/// A host that applies what it is asked to, so a preset's writes actually land.
struct ApplyingHost;

impl nice_plug::context::gui::GuiContextInner for ApplyingHost {
    // A test double has no host to ask for a restart (nice-plug 0.4).
    fn request_restart(&self) {}
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _param: nice_plug::params::internals::ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(
        &self,
        param: nice_plug::params::internals::ParamPtr,
        value: f32,
    ) {
        unsafe {
            param._internal_set_normalized_value(value);
        }
    }
    unsafe fn raw_end_set_parameter(&self, _param: nice_plug::params::internals::ParamPtr) {}
    fn get_state(&self) -> PluginState {
        PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        }
    }
    fn set_state(&self, _: PluginState) {}
}

/// Lands every smoother, as a freshly applied preset must be.
fn settle_smoothers(params: &MxmBucketDelayParams) {
    unsafe {
        for p in [
            &params.time,
            &params.feedback,
            &params.mix,
            &params.tap1,
            &params.tap2,
            &params.tap3,
            &params.tap4,
            &params.tap5,
            &params.tap6,
            &params.spread,
            &params.bias,
            &params.wobble,
        ] {
            p._internal_update_smoother(FS, true);
        }
    }
}

/// Applies one factory preset to a fresh plugin.
fn load(text: &str) -> MxmBucketDelay {
    let mut plugin = MxmBucketDelay::default();
    let parsed = preset::Preset::parse(text, mxm_bucket_delay::CLAP_ID).expect("a factory preset");
    let host = ApplyingHost;
    let setter = ParamSetter::new(&host);
    let (writes, problems) = parsed.resolve(&*plugin.params);
    for problem in &problems {
        println!("  ! {problem}");
    }
    for (_, param, value) in writes {
        param.set(&setter, value);
    }
    settle_smoothers(&plugin.params);
    // Allocates and settles, so the preset is *arrived at* rather than glided into.
    plugin.prepare_for_test(FS, 1);
    plugin
}

fn run(plugin: &mut MxmBucketDelay, input: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(input.len());
    for block in input.chunks(256) {
        let mut l = block.to_vec();
        let mut r = vec![0.0f32; block.len()];
        {
            let mut refs: Vec<&mut [f32]> = vec![&mut l, &mut r];
            plugin.process_block_for_test(&mut refs, Some(120.0));
        }
        out.extend_from_slice(&l);
    }
    out
}

/// How many harmonics the held note carries. Band-limited by hand so the *stimulus* is not itself
/// aliasing into the measurement.
const HARMONICS: usize = 12;

/// A note at 220 Hz decaying at `decay` per second for `seconds`, then `WINDOW_S` of silence.
///
/// `harmonics` at one is a pure sine, which is the **maximum-correlation** input and therefore the
/// worst case for a multi-tap mixer: the root-sum-square normaliser holds the level constant for
/// *decorrelated* returns by design ([`mxm_bucket_delay_dsp::bbd::Bbd::tick`]), so a sine through
/// six taps sums coherently and overshoots by up to `√N`. Program material does not do that, so
/// levels are judged on the rich note and the sine is reported beside it as the headroom a pure
/// tone eats.
fn stimulus(seconds: f32, decay: f32, harmonics: usize) -> Vec<f32> {
    let n = (seconds * FS) as usize;
    let norm: f32 = (1..=harmonics).map(|h| 1.0 / h as f32).sum();
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / FS;
            let wave: f32 = (1..=harmonics)
                .map(|h| (core::f32::consts::TAU * 220.0 * h as f32 * t).sin() / h as f32)
                .sum();
            0.8 * (-t * decay).exp() * wave / norm
        })
        .collect();
    out.resize(n + (WINDOW_S * FS) as usize, 0.0);
    out
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
}

/// How long the tail takes to reach exact zero, what the output measured, and its peak.
///
/// The cap is past [`mxm_bucket_delay::MAX_TAIL_S`], the longest tail the plugin will ever claim,
/// so anything that runs out of it is longer than the product itself is willing to describe.
fn measure(plugin: &mut MxmBucketDelay, input: &[f32]) -> (f32, f32, f32) {
    let played = run(plugin, input);
    let mut peak = played.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
    let mut tail_s = f32::INFINITY;
    let mut elapsed = input.len() as f32 / FS;
    for _ in 0..350 {
        let block = run(plugin, &vec![0.0f32; 4800]);
        peak = peak.max(block.iter().fold(0.0f32, |m, &x| m.max(x.abs())));
        if block.iter().all(|&s| s == 0.0) {
            tail_s = elapsed;
            break;
        }
        elapsed += 0.1;
    }
    (tail_s, rms(&played), peak)
}

fn seconds(tail: f32) -> String {
    if tail.is_finite() {
        format!("{tail:.1} s")
    } else {
        "> 35 s".to_owned()
    }
}

fn main() {
    let pluck = stimulus(NOTE_S, 6.0, HARMONICS);
    let held = stimulus(HELD_S, 0.0, HARMONICS);
    // The pure tone, for the worst-case column alone.
    let sine = stimulus(HELD_S, 0.0, 1);

    println!("mxm-bucket-delay - what each factory preset measures at\n");
    println!("Travel is how far up the Feedback knob the preset sits; the singing point is 90 %.");
    println!("Read the tail column that matches how the preset is played.\n");
    println!("| Preset | Mix | Travel | Loop gain | Plucked | Held | Out RMS | Peak | Sine |");
    println!("|---|---|---|---|---|---|---|---|---|");

    let mut clipping: Vec<(&str, f32)> = Vec::new();
    for (name, text) in preset::FACTORY_FILES {
        let mut plugin = load(text);
        let mix = plugin.params.mix.value();
        let travel = plugin.params.feedback.value();
        let line = plugin.params.line.value().line();
        // Where this sits against the line's own measured sustain threshold. At 1.0 it holds for
        // ever, and the control is calibrated so that is 0.9 of the travel.
        let ratio = feedback_gain(line, travel) / line.sings_at();

        let (pluck_tail, out, peak_a) = measure(&mut plugin, &pluck);
        let (held_tail, _, peak_b) = measure(&mut load(text), &held);
        // The worst case, rendered but not chased: only its peak is wanted.
        let sine_peak = run(&mut load(text), &sine)
            .iter()
            .fold(0.0f32, |m, &x| m.max(x.abs()));

        let peak = peak_a.max(peak_b);
        if peak >= 1.0 {
            clipping.push((*name, peak));
        }
        println!(
            "| {name} | {:.0} % | {:.0} % | {ratio:.2} | {} | {} | {out:.3} | {peak:.2} | {sine_peak:.2} |",
            mix * 100.0,
            travel * 100.0,
            seconds(pluck_tail),
            seconds(held_tail),
        );
    }

    // **The verdict a factory preset has to pass.** Program material, not a pure tone: the sine
    // column is the coherent worst case and is reported rather than enforced, because holding the
    // whole bank under full scale there would mean shipping every multi-tap patch several dB quiet.
    if clipping.is_empty() {
        println!("\nNo preset reaches full scale on a rich note at -2 dBFS.");
    } else {
        println!("\n**Over full scale on program material**, which a factory preset must not be:");
        for (name, peak) in &clipping {
            println!("  {name} peaks at {peak:.2}");
        }
    }

    println!(
        "\nLoop gain at 1.00 is self-oscillation, and the control is calibrated so that happens"
    );
    println!("at 90 % of the travel on every line and at every tap setting.\n");

    // The table a preset is *chosen* from, rather than guessed at. Feedback's curve is calibrated
    // against each line's own measured sustain threshold, so the travel that gives a particular
    // amount of regeneration is not something arithmetic will tell you: it has to be read off.
    println!("Where a Feedback setting puts the loop, per line:\n");
    print!("| Travel |");
    for line in mxm_bucket_delay_dsp::Line::ALL {
        print!(" {line:?} |");
    }
    println!("\n|---|---|---|---|---|");
    for step in 0..=10 {
        let travel = step as f32 / 10.0;
        print!("| {:.0} % |", travel * 100.0);
        for line in mxm_bucket_delay_dsp::Line::ALL {
            print!(" {:.2} |", feedback_gain(line, travel) / line.sings_at());
        }
        println!();
    }
}
