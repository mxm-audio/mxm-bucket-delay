//! Every factory preset, played, and held to what a factory preset owes a player.
//!
//! `preset.rs`'s own tests read the *files* — that the values are the ones the names claim, that
//! the enums are right, that fifty of them exist. This one **renders** them, because the two
//! things a preset can be wrong about are not visible in the numbers:
//!
//! - **It can be inaudible.** `Reverberation` shipped with a loop gain of 0.08 — one pass through
//!   the line — because its `Feedback` had been chosen against a law that was later re-fitted. Every
//!   value in that file was exactly what its author intended and the patch did nothing.
//! - **It can clip.** The tap mixer normalises by `√Σg²`, which holds the level constant for
//!   decorrelated returns and lets correlated ones sum, so a patch with several taps open can run
//!   hotter than its `Mix` suggests.
//!
//! `examples/bucket_delay_preset_audit.rs` is the same measurement with a table instead of assertions, and is
//! what a preset is *designed* against. This is the floor under it.

use mxm_bucket_delay::params::MxmBucketDelayParams;
use mxm_bucket_delay::{MxmBucketDelay, preset};
use nice_plug::params::{InternalParamMut, Param};
use nice_plug::prelude::*;

const FS: f32 = 48_000.0;

/// A host that applies what it is asked to.
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

fn load(text: &str) -> MxmBucketDelay {
    let mut plugin = MxmBucketDelay::default();
    let parsed = preset::Preset::parse(text, mxm_bucket_delay::CLAP_ID).expect("a factory preset");
    let host = ApplyingHost;
    let setter = ParamSetter::new(&host);
    let (writes, _) = parsed.resolve(&*plugin.params);
    for (_, param, value) in writes {
        param.set(&setter, value);
    }
    settle_smoothers(&plugin.params);
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

/// A held note at −2 dBFS with twelve harmonics: **program material, not a pure tone.**
///
/// A sine is the maximum-correlation input, and the mixer is documented as normalising for
/// decorrelated returns, so a sine through six taps sums coherently and overshoots by up to `√N`.
/// Holding the bank under full scale *there* would mean shipping every multi-tap patch several dB
/// quiet to survive a signal nobody plays.
fn held_note(seconds: f32) -> Vec<f32> {
    let n = (seconds * FS) as usize;
    let norm: f32 = (1..=12).map(|h| 1.0 / h as f32).sum();
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / FS;
            let wave: f32 = (1..=12)
                .map(|h| (core::f32::consts::TAU * 220.0 * h as f32 * t).sin() / h as f32)
                .sum();
            0.8 * wave / norm
        })
        .collect();
    out.resize(n + (0.75 * FS) as usize, 0.0);
    out
}

/// **No factory preset may clip on program material**, and none may be NaN or infinite anywhere.
#[test]
fn no_factory_preset_clips_on_an_ordinary_note() {
    let input = held_note(1.0);
    for (name, text) in preset::FACTORY_FILES {
        let mut plugin = load(text);
        let out = run(&mut plugin, &input);
        assert!(
            out.iter().all(|s| s.is_finite()),
            "{name} produced a value that is not finite"
        );
        let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!(
            peak < 1.0,
            "{name} peaks at {peak} on a note at -2 dBFS, which is over full scale"
        );
    }
}

/// **Every preset does something**, which is the one thing a set of plausible numbers cannot
/// promise. The wet a preset adds has to be audible against the dry it replaces — `Reverberation`
/// failed this for a whole revision while every value in its file was the value its author meant.
#[test]
fn every_factory_preset_is_audibly_different_from_the_dry() {
    let input = held_note(1.0);
    for (name, text) in preset::FACTORY_FILES {
        let wet = run(&mut load(text), &input);

        // The same preset with the wet turned all the way down: the dry it would have passed.
        let mut dry_only = load(text);
        unsafe {
            let v = dry_only.params.mix.preview_normalized(0.0);
            let _ = dry_only.params.mix._internal_set_normalized_value(v);
            dry_only.params.mix._internal_update_smoother(FS, true);
        }
        dry_only.prepare_for_test(FS, 1);
        let dry = run(&mut dry_only, &input);

        let difference = wet
            .iter()
            .zip(dry.iter())
            .fold(0.0f32, |m, (w, d)| m.max((w - d).abs()));
        assert!(
            difference > 0.02,
            "{name} is indistinguishable from the dry signal: the most it changes a sample by is {difference}"
        );
    }
}
