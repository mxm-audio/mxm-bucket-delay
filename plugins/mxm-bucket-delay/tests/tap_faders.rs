//! Moving the six tap faders while audio is running.
//!
//! Reported 2026-09-06: the player disappeared while the six tap faders were being played with.
//! No Rust panic reached the log and Windows recorded no fault, which is what an **abort** looks
//! like — and the one thing in this repository that aborts rather than panics is nice-plug's
//! `assert_process_allocs`, which is enabled for every plugin in the workspace and fires when
//! anything allocates on the audio thread.
//!
//! So this file does what a hand does: it drives the six tap parameters continuously while
//! `process` runs, at every line, in both return modes, across a `Line` change and through the
//! transition fades. Anything that allocates, indexes out of range, or produces a value the next
//! stage cannot take has to show up here rather than in a window that vanishes.

use mxm_bucket_delay::MxmBucketDelay;
use nice_plug::params::{InternalParamMut, Param};
use nice_plug::prelude::*;

const FS: f32 = 48_000.0;
const BLOCK: usize = 128;

/// **One input, which is the layout the player negotiates**: the delay declares mono-in/stereo-out
/// first, and that is the configuration a chain behind a mono source picks. The first version of
/// this file used two, which is a different path through `process_block` — `one_input` decides
/// whether the second channel is the source's own or a copy of the first.
fn plugin() -> MxmBucketDelay {
    plugin_with_inputs(1)
}

fn plugin_with_inputs(inputs: usize) -> MxmBucketDelay {
    let mut plugin = MxmBucketDelay::default();
    plugin.prepare_for_test(FS, inputs);
    plugin
}

/// Sets one tap fader the way a host does: a normalised value, then the smoother told about it.
fn set_tap(plugin: &MxmBucketDelay, index: usize, plain: f32) {
    let p = &plugin.params;
    let param = match index {
        0 => &p.tap1,
        1 => &p.tap2,
        2 => &p.tap3,
        3 => &p.tap4,
        4 => &p.tap5,
        _ => &p.tap6,
    };
    unsafe {
        let v = param.preview_normalized(plain);
        let _ = param._internal_set_normalized_value(v);
        param._internal_update_smoother(FS, false);
    }
}

fn set_float(plugin: &MxmBucketDelay, id: &str, plain: f32) {
    let p = &plugin.params;
    unsafe {
        match id {
            "mix" => {
                let v = p.mix.preview_normalized(plain);
                let _ = p.mix._internal_set_normalized_value(v);
                p.mix._internal_update_smoother(FS, true);
            }
            "feedback" => {
                let v = p.feedback.preview_normalized(plain);
                let _ = p.feedback._internal_set_normalized_value(v);
                p.feedback._internal_update_smoother(FS, true);
            }
            "spread" => {
                let v = p.spread.preview_normalized(plain);
                let _ = p.spread._internal_set_normalized_value(v);
                p.spread._internal_update_smoother(FS, true);
            }
            other => unreachable!("{other}"),
        }
    }
}

fn set_enum<T: nice_plug::params::enums::Enum + PartialEq + Copy>(param: &EnumParam<T>, value: T) {
    unsafe {
        let v = param.preview_normalized(value);
        let _ = param._internal_set_normalized_value(v);
    }
}

/// One block of audio, with the faders wherever they have been put.
fn render(plugin: &mut MxmBucketDelay, block: usize, source: f32) -> (Vec<f32>, Vec<f32>) {
    let mut left: Vec<f32> = (0..block)
        .map(|i| source * (core::f32::consts::TAU * 220.0 * i as f32 / FS).sin())
        .collect();
    let mut right = vec![0.0f32; block];
    {
        let mut refs: Vec<&mut [f32]> = vec![&mut left, &mut right];
        plugin.process_block_for_test(&mut refs, Some(120.0));
    }
    (left, right)
}

/// **The report, as a test.** Six faders swept continuously while audio runs, at every line, in
/// both return modes, with `Spread` and `Feedback` moving under them — because that is what a hand
/// on six knobs does, and because the tap positions, the loop gain and the takeoff sum all change
/// together when they move.
#[test]
fn sweeping_the_six_tap_faders_under_audio_is_survivable() {
    for inputs in [1, 2] {
        for line in [
            mxm_bucket_delay::params::LineChoice::S1024,
            mxm_bucket_delay::params::LineChoice::S3328,
            mxm_bucket_delay::params::LineChoice::S4096,
            mxm_bucket_delay::params::LineChoice::S8192,
        ] {
            for ret in [
                mxm_bucket_delay::params::ReturnChoice::Mix,
                mxm_bucket_delay::params::ReturnChoice::Tail,
            ] {
                let mut plugin = plugin_with_inputs(inputs);
                set_enum(&plugin.params.line, line);
                set_enum(&plugin.params.return_mode, ret);
                set_float(&plugin, "mix", 1.0);
                set_float(&plugin, "feedback", 0.7);

                for block in 0..400 {
                    // Every fader moving on every block, out of phase with each other, over the whole
                    // of their travel — including all six at the top at once.
                    for tap in 0..6 {
                        let phase = block as f32 * 0.09 + tap as f32 * 0.7;
                        set_tap(&plugin, tap, 0.5 + 0.5 * phase.sin());
                    }
                    if block % 37 == 0 {
                        set_float(&plugin, "spread", (block % 200) as f32 / 100.0);
                    }
                    let (l, r) = render(&mut plugin, BLOCK, if block < 200 { 0.8 } else { 0.0 });
                    for (i, (&a, &b)) in l.iter().zip(r.iter()).enumerate() {
                        assert!(
                            a.is_finite() && b.is_finite(),
                            "{line:?} {ret:?}: block {block} sample {i} is not finite: {a} / {b}"
                        );
                        assert!(
                            a.abs() < 64.0 && b.abs() < 64.0,
                            "{line:?} {ret:?}: block {block} sample {i} is {a} / {b}"
                        );
                    }
                }
            }
        }
    }
}

/// The faders moving **across a `Line` change**, which swaps the chip under them and re-derives
/// every tap position while the fades run.
#[test]
fn the_faders_survive_a_line_change_under_them() {
    let mut plugin = plugin();
    set_float(&plugin, "mix", 1.0);
    set_float(&plugin, "feedback", 0.6);
    for tap in 0..6 {
        set_tap(&plugin, tap, 1.0);
    }

    let lines = [
        mxm_bucket_delay::params::LineChoice::S1024,
        mxm_bucket_delay::params::LineChoice::S8192,
        mxm_bucket_delay::params::LineChoice::S3328,
        mxm_bucket_delay::params::LineChoice::S4096,
    ];
    for block in 0..600 {
        if block % 50 == 0 {
            set_enum(&plugin.params.line, lines[(block / 50) % lines.len()]);
        }
        for tap in 0..6 {
            let phase = block as f32 * 0.13 + tap as f32 * 1.1;
            set_tap(&plugin, tap, 0.5 + 0.5 * phase.cos());
        }
        let (l, r) = render(&mut plugin, BLOCK, 0.7);
        assert!(
            l.iter().chain(r.iter()).all(|s| s.is_finite()),
            "block {block} produced a non-finite sample"
        );
    }
}

/// All six at the top with the feedback in the singing region — the loudest the loop can be asked
/// to be, and the setting somebody reaching for "what do these do" arrives at.
#[test]
fn all_six_wide_open_into_a_singing_loop_stays_bounded() {
    let mut plugin = plugin();
    set_float(&plugin, "mix", 1.0);
    set_float(&plugin, "feedback", 1.0);
    for tap in 0..6 {
        set_tap(&plugin, tap, 1.0);
    }
    let mut worst = 0.0f32;
    for block in 0..2000 {
        let (l, r) = render(&mut plugin, BLOCK, if block < 100 { 0.9 } else { 0.0 });
        for &s in l.iter().chain(r.iter()) {
            assert!(s.is_finite(), "block {block}: {s}");
            worst = worst.max(s.abs());
        }
    }
    assert!(worst < 64.0, "the loop reached {worst}");
}
