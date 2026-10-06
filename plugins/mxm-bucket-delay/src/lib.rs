//! mxm-bucket-delay — a bucket brigade delay, built from the device rather than from a box.
//!
//! The device is `research:effects/bucket-brigade-delay.md`; the DSP is
//! `crates/mxm-bucket-delay-dsp`, this product's own. Not affiliated with or endorsed by Panasonic.
//!
//! # The reference is a device family, not a pedal, and that is the unusual thing here
//!
//! Every other effect in the collection copies one machine. This one copies a **component** — the
//! MN30xx bucket brigade — and the character comes from a chain of consequences the component
//! imposes on whatever is built around it. Copying a DM-2 or a Memory Man was tried first and
//! abandoned when the catalogue turned up the **MN3011**: a six-tap part sold as a reverb, whose
//! taps are deliberately *"not in multiple proportion with each other"*. The most interesting thing
//! the device can do was never in a famous pedal, and picking a pedal would have hidden it behind
//! that pedal's three knobs.
//!
//! # The thesis, because it decides everything
//!
//! In a digital delay, time, brightness, noise and grit are four independent choices. In a bucket
//! brigade they are **one**, because all four follow from the clock: the only way to lengthen the
//! delay is to slow the sampler. `Time` is therefore not a delay control — it is the clock, and
//! turning it bends the pitch of what is already in the line, the way a tape machine does.
//!
//! # Mix at zero is Off, and doing nothing costs nothing
//!
//! No On switch, as there is none on the chorus or the spring. At zero the wet fades out, the lines
//! are **emptied rather than frozen** — a frozen line spills a stale repeat from audio minutes old
//! when the level returns — and from the next block the core is not run at all.
//!
//! **And a tail that has ended parks the same way.** The chip's own noise never stops, so a core
//! that merely cleared its line would refill it with a noise floor within one lap and never reach
//! exact zero; the DSP stops instead, which is what makes the silence exact and the CPU claim true.
//!
//! # Waking without input
//!
//! A parked, silent instance whose `Feedback` — or a tap fader, or `Return` — is raised into the
//! self-starting region has to start singing on the strength of a parameter change alone. The wake
//! condition is **the loop gain crossing the threshold**, whichever control moved it, and this
//! plugin therefore never skips a block while the gain is in that region. MXM Player's FX chain
//! (`apps/mxm-player`, in mxm-player) had this exact defect and fixed it; the product does not
//! rely on a host being more generous than the player was.
//!
//! # Two layouts
//!
//! One in, two out is the default: one source driving both lines, its dry copied to both outputs.
//! Two in, two out keeps each channel's own source and its own dry. **Never summed** — summing
//! would silence anti-correlated material, which is a defect rather than a wart.

/// The plugin's name, and the **only** place it is written in this crate.
///
/// A macro rather than a `const` because [`CLAP_ID`] is built with `concat!`, which takes literals.
macro_rules! plugin_name {
    () => {
        "mxm-bucket-delay"
    };
}

/// What the host displays.
pub const NAME: &str = plugin_name!();

/// The permanent CLAP identifier.
///
/// **Deliberately assembled from [`plugin_name!`] and not from `CARGO_PKG_NAME`.** Deriving it from
/// the package name would mean a future `git mv` of this directory silently changed the plugin's
/// permanent identity — no compile error, no failing test, and every preset and saved project
/// written under the old id orphaned.
///
/// Reverse DNS on a domain the project owns, which is why it **cannot** collide with another
/// vendor's however similar a display name looks. `plugins/AGENTS.md` used to require a check
/// against the published CLAP lists; the owner's ruling of 2026-09-05 is that the check was
/// meaningless, and it has been corrected there rather than worked around here.
pub const CLAP_ID: &str = concat!("dk.mxm.", plugin_name!());

pub mod editor;
// **Public**, unlike either sibling's: this plugin's parameter module carries facts about the
// circuit as constants — the reference reverberation ladder among them — and a constant that
// documents a circuit is worth reading from outside the crate. `preset` is public for the same
// kind of reason.
pub mod params;
pub mod preset;
pub mod telemetry;

use mxm_bucket_delay_dsp::bbd::feedback_gain;
use mxm_bucket_delay_dsp::unit::{SNAP_HOLD_S, SNAP_LEVEL};
use mxm_bucket_delay_dsp::{Controls, Core, Shape};
use nice_plug::plugin::ParamValue;
use nice_plug::prelude::*;
use params::{ChangeChoice, MxmBucketDelayParams};
use std::sync::Arc;
use telemetry::Telemetry;

/// The longest tail the plugin will ever claim, whatever the feedback.
///
/// **Chosen**, and `mxm-folded-spring`'s figure. Near the threshold the arithmetic below runs away
/// toward forever, and a host asked to keep a plugin awake forever is a host that never sleeps.
/// Half a minute is past any musical tail and short of forever.
pub const MAX_TAIL_S: f32 = 30.0;

/// How fast the delay time glides to a new target, as a one-pole time constant in seconds.
///
/// **The glide is the product**, not a smoother fitted to remove zipper noise: time *is* clock
/// rate, so a moving target drags the pitch of everything already in the line. This is the rate at
/// which a synced `Change = Glide` carries a tail into a new tempo, and the rate at which a turned `Time` knob
/// bends it. **Chosen**: fast enough to feel like a control, slow enough to hear as a slide.
pub const TIME_GLIDE_S: f32 = 0.08;

pub struct MxmBucketDelay {
    pub params: Arc<MxmBucketDelayParams>,
    /// The only channel to the editor.
    telemetry: Arc<Telemetry>,
    core: Core,
    sample_rate: f32,
    /// Input channels in the negotiated layout: one or two.
    input_channels: usize,
    /// The delay in force, which glides toward whatever the controls and the transport ask for.
    time_s: f32,
    /// The wobble's rate its sync resolved for this block, or `None` for the free rate.
    synced_rate_hz: Option<f32>,
}

impl Default for MxmBucketDelay {
    fn default() -> Self {
        Self {
            params: Arc::new(MxmBucketDelayParams::default()),
            synced_rate_hz: None,
            telemetry: Telemetry::shared(),
            core: Core::new(48_000.0),
            sample_rate: 48_000.0,
            input_channels: 1,
            time_s: params::DEFAULT_TIME_S,
        }
    }
}

impl MxmBucketDelay {
    /// What `activate` does, and what a test does in its place.
    fn prepare(&mut self, sample_rate: f32, input_channels: usize) {
        // Forget the last activation's tempo too: nice-plug resets right after activating, and a
        // division resolved from a tempo the host may since have changed would seed the engine.
        self.telemetry.publish_tempo(None);
        // A restored state is resolved afresh by the next block: activation must not seed the
        // engine with the division the previous state was synced to.
        self.synced_rate_hz = None;
        self.sample_rate = sample_rate;
        self.input_channels = input_channels;
        // Allocates both lines and both reverse windows. Here, never in `process`.
        self.core = Core::new(sample_rate);
        self.settle();
    }

    /// Lands every control at its target with no glide, as a freshly applied preset must be.
    fn settle(&mut self) {
        // The delay in force: the knob free, its subdivision synced to the last tempo seen.
        self.time_s = self.target_time(self.telemetry.tempo());
        self.core.set_shape(self.shape());
        self.core.set_controls(self.controls());
        self.core.reset();
    }

    fn shape(&self) -> Shape {
        Shape {
            line: self.params.line.value().line(),
            routing: self.params.routing.value().routing(),
            reverse: self.params.reverse.value(),
            return_mode: self.params.return_mode.value().mode(),
        }
    }

    /// The controls as they stand this sample, with the gains taken from their smoothers.
    fn controls(&self) -> Controls {
        let p = &self.params;
        let line = p.line.value().line();
        Controls {
            time_s: self.time_s as f64,
            // The control's travel through this line's own measured curve. Every line therefore
            // sings at the same place on the knob, which one constant for all four could not do.
            feedback: feedback_gain(line, p.feedback.smoothed.next()),
            mix: p.mix.smoothed.next(),
            taps: [
                p.tap1.smoothed.next(),
                p.tap2.smoothed.next(),
                p.tap3.smoothed.next(),
                p.tap4.smoothed.next(),
                p.tap5.smoothed.next(),
                p.tap6.smoothed.next(),
            ],
            spread: p.spread.smoothed.next(),
            bias: p.bias.smoothed.next(),
            wobble: p.wobble.smoothed.next(),
            // Synced, the wobble runs at its division; the free smoother is advanced as it is with sync off,
            // so turning sync off lands on the knob rather than an old rate.
            wobble_rate_hz: {
                let free = p.rate.smoothed.next();
                self.synced_rate_hz.unwrap_or(free)
            },
            filter: p.filter.value().mode(),
        }
    }

    /// The delay the transport and the controls are asking for.
    ///
    /// **`Time` is the delay control whether or not `Sync` is on**, and that is the 2026-09-06
    /// correction: free the knob is seconds, and synced the same knob
    /// picks a subdivision — the one its position asks for, clamped to what this tempo and this
    /// chip can reach. It used to select nothing at all while a separate `Division` did the work,
    /// which the owner met as *"when time is set to snap it is hardcoded to 1/8"*.
    ///
    /// If the host has no tempo to give it is the knob again, because a delay that fell silent in a
    /// host without a transport would be broken rather than honest.
    pub fn target_time(&self, tempo: Option<f64>) -> f32 {
        self.params.target_time(tempo)
    }

    /// The wobble's rate while its sync follows the host, or `None` for its free rate: the Rate's
    /// modulated position picks a division on the LFO ladder (`params::RATE_SYNC`).
    fn synced_rate(&self, tempo: Option<f64>) -> Option<f32> {
        let rate = &self.params.rate;
        params::RATE_SYNC
            .resolve(
                self.params.rate_sync.value(),
                tempo,
                rate.modulated_normalized_value(),
                f64::from(rate.preview_plain(0.0)),
                f64::from(rate.preview_plain(1.0)),
            )
            .map(|hz| hz as f32)
    }

    /// How long the loop needs to fall below the snap threshold, at the gain **and the clock** in
    /// force.
    ///
    /// **Recomputed every block rather than latched**, which is what makes it follow `Feedback`,
    /// `Line`, `Return`, a tap fader, `Time`, `Sync` and `Change` without naming any of them: a
    /// tail lengthened by any of them would otherwise be truncated while the host counted down an
    /// estimate taken before the change.
    /// The loop gain the controls are asking for, in the units the sustain thresholds are measured
    /// in.
    ///
    /// **`Feedback` alone.** It used to be the control times the takeoff sum, which made the tap
    /// faders re-scale the knob under the player's hand — see `applied_feedback` in the DSP. The
    /// normalisation moved that into the loop, so what the control says is what the loop does.
    fn loop_gain_now(&self) -> f32 {
        let p = &self.params;
        feedback_gain(p.line.value().line(), p.feedback.value())
    }

    fn tail_samples(&self) -> u32 {
        let g = self.loop_gain_now();
        // Counted from now, so the lap in force is enough for the line: what is left of a sample's
        // journey through the buckets is never longer. A click still waiting in the reverse window
        // is not in the buckets yet, and its wait goes in front.
        let lap = self.core.lap_seconds() as f32;
        let laps = if g <= 1e-4 {
            1.0
        } else {
            (SNAP_LEVEL.ln() / g.ln()).max(1.0)
        };
        let reverse = self.core.reverse_hold_seconds() as f32;
        let seconds = (reverse + lap * (laps + 1.0) + SNAP_HOLD_S).min(MAX_TAIL_S);
        (seconds * self.sample_rate) as u32
    }

    /// Whether the loop is generating rather than decaying. A tail may be truncated by a host; a
    /// generator must not be, so this is `Normal` and never `Tail`.
    fn singing(&self) -> bool {
        self.loop_gain_now() >= self.params.line.value().line().sings_at()
    }

    /// What `activate` does, reachable from an integration test that has no host.
    pub fn prepare_for_test(&mut self, sample_rate: f32, input_channels: usize) {
        self.prepare(sample_rate, input_channels);
    }

    /// One block, reachable from an integration test. The same entry `process` uses.
    pub fn process_block_for_test(
        &mut self,
        channels: &mut [&mut [f32]],
        tempo: Option<f64>,
    ) -> ProcessStatus {
        self.process_block(channels, tempo)
    }

    fn process_block(&mut self, channels: &mut [&mut [f32]], tempo: Option<f64>) -> ProcessStatus {
        let Some(first) = channels.first() else {
            return ProcessStatus::Normal;
        };
        let n = first.len();
        let one_input = self.input_channels < 2 || channels.len() < 2;

        // Before the skip, not after: the panel names the subdivision `Time` is selecting, and a
        // parked instance is exactly when somebody is looking at the panel rather than playing.
        self.telemetry.publish_tempo(tempo);
        self.synced_rate_hz = self.synced_rate(tempo);

        self.core.set_shape(self.shape());

        // The activity rule, and the input's flush in the same pass. A subnormal is below anything
        // a converter carries and on x86 slow enough that a validator measures it. A non-finite
        // sample carries nothing either, and is neither passed on nor counted as activity: one that
        // reached the loop would stay there, and one counted as activity would end a tail's status.
        let inputs = self.input_channels.min(channels.len());
        let mut active = false;
        for ch in channels[..inputs].iter_mut() {
            for s in ch[..n].iter_mut() {
                if !s.is_finite() || s.abs() < f32::MIN_POSITIVE {
                    *s = 0.0;
                } else {
                    active = true;
                }
            }
        }

        // **Never skip a block while the loop can start itself.** The wake condition is the gain
        // crossing, whichever control moved it, and a parked instance that skipped here could never
        // sing from its own noise.
        let line = self.params.line.value().line();
        let can_self_start = self.loop_gain_now() >= line.starts_at();
        if !active && self.core.is_parked() && !can_self_start {
            if one_input && channels.len() > 1 {
                let (head, rest) = channels.split_at_mut(1);
                for ch in rest.iter_mut() {
                    ch[..n].copy_from_slice(&head[0][..n]);
                }
            }
            return ProcessStatus::Normal;
        }

        let target = self.target_time(tempo);
        let glide = 1.0 - (-1.0 / (TIME_GLIDE_S * self.sample_rate)).exp();
        // Change applies only while Time follows the tempo: free, the time glides as a turned knob
        // does, exactly as before the split.
        let snap = self.params.sync.value() && self.params.change.value() == ChangeChoice::Snap;

        let mut peak = 0.0f32;
        let mut ring = 0.0f32;
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            // The glide, which is what `Change = Glide` means and what a turned knob does. `Snap`
            // takes the new time at once: in time, and audibly digital.
            if snap {
                self.time_s = target;
            } else {
                self.time_s += (target - self.time_s) * glide;
            }

            let controls = self.controls();
            self.core.set_controls(controls);

            let dry_l = channels[0][i];
            let dry_r = if one_input { dry_l } else { channels[1][i] };
            let (out_l, out_r) = self.core.process(dry_l, dry_r, one_input);

            // **The wet, against the dry the crossfade actually left.** `out - dry` was right while
            // the wet was added on top of a whole dry; under a crossfade it also counts the dry the
            // mix took away, so at `Mix = 1` a dry signal alone would light the constellation.
            let dry_gain = 1.0 - controls.mix;
            ring = ring
                .max((out_l - dry_gain * dry_l).abs())
                .max((out_r - dry_gain * dry_r).abs());
            peak = peak.max(out_l.abs()).max(out_r.abs());
            channels[0][i] = out_l;
            if channels.len() > 1 {
                channels[1][i] = out_r;
            }
        }

        // Anything beyond the pair the layout declares is a copy of the left, as nice-plug's
        // zero-filled extras would otherwise be silence.
        if channels.len() > 2 {
            let (head, rest) = channels.split_at_mut(1);
            for ch in rest.iter_mut().skip(1) {
                ch[..n].copy_from_slice(&head[0][..n]);
            }
        }

        self.telemetry.publish_peak(peak);
        self.telemetry.publish_ring(ring);

        if active || self.singing() || self.core.is_parked() {
            ProcessStatus::Normal
        } else {
            ProcessStatus::Tail(self.tail_samples())
        }
    }
}

/// Rewrites an old three-way `sync` (`free`, `glide`, `snap`) into the Sync switch and Change.
/// Anything else — a state saved since the split — is left alone.
/// Whether it rewrote one.
fn migrate_sync(params: &mut std::collections::BTreeMap<String, ParamValue>) -> bool {
    let Some(ParamValue::String(old)) = params.get("sync").cloned() else {
        return false;
    };
    // Only the three the old enum had; anything else is left for nice-plug to reject as it would.
    if !matches!(old.as_str(), "free" | "glide" | "snap") {
        return false;
    }
    params.insert("sync".to_owned(), ParamValue::Bool(old != "free"));
    params.entry("change".to_owned()).or_insert_with(|| {
        ParamValue::String(if old == "snap" { "snap" } else { "glide" }.to_owned())
    });
    true
}

/// The same split for a loaded preset's baseline, which stored the three-way Sync as its normalised
/// position — Free 0, Glide ½, Snap 1 — so an unchanged preset does not reopen as Modified.
fn migrate_sync_baseline(fields: &mut std::collections::BTreeMap<String, String>) {
    use nice_plug::params::persist::{deserialize_field, serialize_field};
    let Some(serialized) = fields.get_mut("preset") else {
        return;
    };
    let Ok(mut identity) = deserialize_field::<mxm_preset::PresetIdentity>(serialized) else {
        return;
    };
    let Some(&old) = identity.baseline.get("sync") else {
        return;
    };
    if identity.baseline.contains_key("change") {
        return;
    }
    identity
        .baseline
        .insert("sync".to_owned(), if old > 0.25 { 1.0 } else { 0.0 });
    identity
        .baseline
        .insert("change".to_owned(), if old > 0.75 { 1.0 } else { 0.0 });
    if let Ok(canonical) = serialize_field(&identity) {
        *serialized = canonical;
    }
}

impl Plugin for MxmBucketDelay {
    const NAME: &'static str = crate::NAME;
    const VENDOR: &'static str = "mxm";
    const URL: &'static str = "https://mxm.dk";
    const EMAIL: &'static str = "plugins@mxm.dk";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// The pair `mxm-chorus-06` declares, and for the same reason: **both have a stereo output**,
    /// because two lines with their modulation in antiphase is what the stereo modes are. One in
    /// drives both lines; two in keeps each channel's own source.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];

    /// An effect: no note port. Nothing here is played.
    const MIDI_INPUT: MidiConfig = MidiConfig::None;

    /// The smoothers advance per sample, and every transition is a fade the DSP runs itself.
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type Editor = editor::MxmBucketDelayEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        editor::create(self.params.clone(), self.telemetry.clone())
    }

    /// **A project saved before the tempo-sync split** (`plans/plan-tempo-sync-controls.md`,
    /// Revision 2) stored `sync` as the three-way Free/Glide/Snap. It reads as Sync off, on, on, and
    /// the Glide or Snap it chose becomes Change — so the project sounds as it was saved.
    ///
    /// A loaded preset's baseline is split the same way, and Rate sync, added then too, restores Off
    /// (`mxm_preset::add_switches_off`).
    fn filter_state(state: &mut PluginState) {
        if migrate_sync(&mut state.params) {
            migrate_sync_baseline(&mut state.fields);
        }
        mxm_preset::add_switches_off(state, crate::preset::RESTORED_OFF);
    }

    fn activate(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        let inputs = audio_io_layout
            .main_input_channels
            .map_or(1, |c| c.get() as usize);
        self.prepare(buffer_config.sample_rate, inputs);
        true
    }

    fn reset(&mut self) {
        // A host resets without a callback between (a bypass, a transport restart), and a parameter
        // flush may have moved a sync meanwhile: re-resolve every sync from the parameters as they
        // stand and the last tempo seen, so nothing is seeded from the previous division.
        self.synced_rate_hz = self.synced_rate(self.telemetry.tempo());
        self.settle();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let tempo = context.transport().tempo;
        self.process_block(buffer.as_slice(), tempo)
    }
}

impl ClapPlugin for MxmBucketDelay {
    /// Permanent. Reverse DNS of a domain the project owns.
    const CLAP_ID: &'static str = CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "A bucket-brigade (BBD) delay with six taps and four line lengths, from short doubling to long, gritty repeats",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Delay,
        ClapFeature::Stereo,
    ];
}

nice_export_clap!(MxmBucketDelay);

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::params::{InternalParamMut, Param};

    const FS: f32 = 48_000.0;

    fn plugin(inputs: usize) -> MxmBucketDelay {
        let mut plugin = MxmBucketDelay::default();
        for id in ALL_SMOOTHED {
            update(&plugin, id);
        }
        plugin.prepare(FS, inputs);
        plugin
    }

    /// `bundler.toml` names the same plugin this crate does — the one place the name is duplicated
    /// outside this crate, read by `xtask` at bundle time and never by the plugin.
    #[test]
    fn the_bundle_is_named_after_this_plugin() {
        assert_eq!(CLAP_ID, format!("dk.mxm.{NAME}"));
        mxm_plugin_test::bundle::is_named(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"), NAME);
    }

    const ALL_SMOOTHED: [&str; 12] = [
        "time", "feedback", "mix", "tap1", "tap2", "tap3", "tap4", "tap5", "tap6", "spread",
        "bias", "wobble",
    ];

    fn update(plugin: &MxmBucketDelay, id: &str) {
        let p = &plugin.params;
        unsafe {
            match id {
                "time" => p.time._internal_update_smoother(FS, true),
                "feedback" => p.feedback._internal_update_smoother(FS, true),
                "mix" => p.mix._internal_update_smoother(FS, true),
                "tap1" => p.tap1._internal_update_smoother(FS, true),
                "tap2" => p.tap2._internal_update_smoother(FS, true),
                "tap3" => p.tap3._internal_update_smoother(FS, true),
                "tap4" => p.tap4._internal_update_smoother(FS, true),
                "tap5" => p.tap5._internal_update_smoother(FS, true),
                "tap6" => p.tap6._internal_update_smoother(FS, true),
                "spread" => p.spread._internal_update_smoother(FS, true),
                "bias" => p.bias._internal_update_smoother(FS, true),
                "wobble" => p.wobble._internal_update_smoother(FS, true),
                "rate" => p.rate._internal_update_smoother(FS, true),
                _ => unreachable!("{id}"),
            }
        }
    }

    fn set_float(plugin: &MxmBucketDelay, id: &str, plain: f32) {
        let p = &plugin.params;
        unsafe {
            match id {
                "time" => {
                    let v = p.time.preview_normalized(plain);
                    let _ = p.time._internal_set_normalized_value(v);
                }
                "feedback" => {
                    let v = p.feedback.preview_normalized(plain);
                    let _ = p.feedback._internal_set_normalized_value(v);
                }
                "mix" => {
                    let v = p.mix.preview_normalized(plain);
                    let _ = p.mix._internal_set_normalized_value(v);
                }
                _ => unreachable!("{id}"),
            }
        }
        update(plugin, id);
    }

    /// Time follows the tempo, with `change` at a new tempo or subdivision.
    fn sync(plugin: &MxmBucketDelay, change: ChangeChoice) {
        let p = &plugin.params;
        unsafe {
            let _ = p.sync._internal_set_normalized_value(1.0);
            let v = p.change.preview_normalized(change);
            let _ = p.change._internal_set_normalized_value(v);
        }
    }

    /// Runs `input` through the plugin and returns both output channels.
    fn run(plugin: &mut MxmBucketDelay, input: &[f32], block: usize) -> (Vec<f32>, Vec<f32>) {
        let (mut left, mut right) = (Vec::new(), Vec::new());
        for chunk in input.chunks(block) {
            let mut l = chunk.to_vec();
            let mut r = vec![0.0; chunk.len()];
            {
                let mut refs: Vec<&mut [f32]> = vec![&mut l, &mut r];
                plugin.process_block(&mut refs, Some(120.0));
            }
            left.extend_from_slice(&l);
            right.extend_from_slice(&r);
        }
        (left, right)
    }

    fn tone(n: usize, hz: f32, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (core::f32::consts::TAU * hz * i as f32 / FS).sin())
            .collect()
    }

    /// **Off is off**, and the proof is bit-exact: at Mix zero the output is the dry input, on
    /// both channels of the one-input layout.
    #[test]
    fn at_mix_zero_the_output_is_the_dry_input_to_the_bit() {
        let mut plugin = plugin(1);
        set_float(&plugin, "mix", 0.0);
        // Let the fade to Off run out first.
        run(&mut plugin, &vec![0.0; 4800], 256);

        let probe = tone(4800, 330.0, 0.5);
        let (l, r) = run(&mut plugin, &probe, 256);
        assert_eq!(l, probe, "the dry did not pass through untouched");
        assert_eq!(r, probe, "the copy to the second output is not the dry");
    }

    /// A delay delays: something arrives about one delay time after the input, and nothing arrives
    /// before it.
    #[test]
    fn what_goes_in_comes_back_a_delay_time_later() {
        let mut plugin = plugin(1);
        set_float(&plugin, "time", 0.1);
        set_float(&plugin, "mix", 1.0);
        set_float(&plugin, "feedback", 0.0);
        plugin.settle();

        let mut input = vec![0.0f32; (0.4 * FS) as usize];
        for (i, s) in input.iter_mut().enumerate().take(64) {
            *s = 0.9 * (1.0 - i as f32 / 64.0);
        }
        let (l, _) = run(&mut plugin, &input, 128);

        // **Mix is a crossfade**, so at one the dry is gone and the output *is* the wet. Subtracting
        // the input here — which is what this read before the level became a mix — would have measured
        // the dry back into the answer and called the input's own click an early arrival.
        let wet: Vec<f32> = l.iter().map(|o| o.abs()).collect();
        let before: f32 = wet[..(0.05 * FS) as usize]
            .iter()
            .fold(0.0, |m, &x| m.max(x));
        let around: f32 = wet[(0.09 * FS) as usize..(0.13 * FS) as usize]
            .iter()
            .fold(0.0, |m, &x| m.max(x));
        assert!(
            before < 1e-4,
            "something arrived before the delay time: {before}"
        );
        assert!(
            around > 1e-3,
            "nothing came back at the delay time: {around}"
        );
    }

    /// The tail estimate is **recomputed**, so raising Feedback under a live tail lengthens what the
    /// plugin owes the host rather than leaving it counting down an old number.
    #[test]
    fn the_tail_the_host_is_told_follows_the_feedback_control() {
        let plugin = plugin(1);
        set_float(&plugin, "feedback", 0.2);
        let short = plugin.tail_samples();
        set_float(&plugin, "feedback", 0.8);
        let long = plugin.tail_samples();
        assert!(long > short, "{long} is not longer than {short}");
    }

    /// **A non-finite input sample is neither audio nor activity.** The input pass zeroes it, so
    /// the block is exactly what a zero would have made, and the status a live tail reports is not
    /// turned into `Normal` by a sample that carries nothing.
    #[test]
    fn a_non_finite_input_sample_is_neither_audio_nor_activity() {
        fn block(
            plugin: &mut MxmBucketDelay,
            input: &[f32],
        ) -> (Vec<f32>, Vec<f32>, ProcessStatus) {
            let mut l = input.to_vec();
            let mut r = vec![0.0; input.len()];
            let status = {
                let mut refs: Vec<&mut [f32]> = vec![&mut l, &mut r];
                plugin.process_block(&mut refs, Some(120.0))
            };
            (l, r, status)
        }
        fn same(
            p: &(Vec<f32>, Vec<f32>, ProcessStatus),
            r: &(Vec<f32>, Vec<f32>, ProcessStatus),
        ) -> bool {
            let bits =
                |a: &[f32], b: &[f32]| a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits());
            bits(&p.0, &r.0) && bits(&p.1, &r.1) && p.2 == r.2
        }

        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut poisoned = plugin(1);
            let mut reference = plugin(1);
            for p in [&mut poisoned, &mut reference] {
                set_float(p, "time", 0.05);
                set_float(p, "mix", 0.5);
                set_float(p, "feedback", 0.5);
                p.settle();
            }
            for chunk in tone((0.2 * FS) as usize, 220.0, 0.8).chunks(256) {
                assert!(same(
                    &block(&mut poisoned, chunk),
                    &block(&mut reference, chunk)
                ));
            }
            let silent = vec![0.0f32; 256];
            let quiet = block(&mut reference, &silent);
            assert!(
                matches!(quiet.2, ProcessStatus::Tail(_)),
                "the reference is not in a tail: {:?}",
                quiet.2
            );
            assert!(same(&block(&mut poisoned, &silent), &quiet));

            let mut spiked = silent.clone();
            spiked[17] = bad;
            let p = block(&mut poisoned, &spiked);
            let r = block(&mut reference, &silent);
            assert!(same(&p, &r), "{bad}: {:?} against {:?}", p.2, r.2);

            for (k, chunk) in tone((0.5 * FS) as usize, 330.0, 0.8)
                .chunks(256)
                .enumerate()
            {
                let p = block(&mut poisoned, chunk);
                let r = block(&mut reference, chunk);
                assert!(same(&p, &r), "{bad}: block {k} after it did not recover");
            }
        }
    }

    /// **Turning Time down under a repeat keeps the tail until the repeat has played.** A click at
    /// 2 s, then Time to 0.3 s three quarters of a lap later: the click is still in the line, so
    /// the host must go on being told `Tail` and the repeat must come out, glided knob and all.
    #[test]
    fn shortening_time_keeps_the_tail_until_the_repeat_has_played() {
        const BLOCK: usize = 256;
        let mut plugin = plugin(1);
        set_float(&plugin, "time", 2.0);
        set_float(&plugin, "mix", 1.0);
        set_float(&plugin, "feedback", 0.0);
        plugin.settle();

        let mut input = vec![0.0f32; (1.5 * FS) as usize];
        for (i, s) in input.iter_mut().enumerate().take(64) {
            *s = 0.9 * (1.0 - i as f32 / 64.0);
        }
        let (early, _) = run(&mut plugin, &input, BLOCK);
        assert!(
            early[64..].iter().all(|s| s.abs() < 1e-3),
            "the repeat arrived early"
        );

        set_float(&plugin, "time", 0.3);
        let mut heard = 0.0f32;
        for k in 0..FS as usize / BLOCK {
            let mut l = vec![0.0f32; BLOCK];
            let mut r = vec![0.0f32; BLOCK];
            let status = {
                let mut refs: Vec<&mut [f32]> = vec![&mut l, &mut r];
                plugin.process_block(&mut refs, Some(120.0))
            };
            if heard <= 100.0 * SNAP_LEVEL {
                assert!(
                    matches!(status, ProcessStatus::Tail(_)),
                    "block {k}: {status:?} with the repeat still in the line"
                );
            }
            heard = l.iter().chain(&r).fold(heard, |m, &x| m.max(x.abs()));
        }
        assert!(
            heard > 100.0 * SNAP_LEVEL,
            "the repeat was snapped away: {heard:e}"
        );
    }

    /// **The tail the host is told covers the reverse window.** At 250 ms a click played into a
    /// quiet instance waits up to two windows before the line sees it, and leaves a lap after that.
    /// The tail reported once the input has stopped has to reach past the reversed click, or a host
    /// counting it down stops the plugin with the click still held.
    #[test]
    fn the_tail_the_host_is_told_covers_a_click_held_in_the_reverse_window() {
        const BLOCK: usize = 256;
        let mut plugin = plugin(1);
        set_float(&plugin, "time", 0.25);
        set_float(&plugin, "mix", 1.0);
        set_float(&plugin, "feedback", 0.0);
        unsafe {
            let v = plugin.params.reverse.preview_normalized(true);
            let _ = plugin.params.reverse._internal_set_normalized_value(v);
        }
        plugin.settle();
        run(&mut plugin, &vec![0.0; FS as usize], BLOCK);
        assert!(plugin.core.is_parked(), "the instance did not start quiet");

        let mut input = vec![0.0f32; BLOCK];
        for (i, s) in input.iter_mut().enumerate().take(64) {
            *s = 0.9 * (1.0 - i as f32 / 64.0);
        }
        run(&mut plugin, &input, BLOCK);

        let mut told: Option<(usize, u32)> = None;
        let mut last_heard = None;
        for k in 0..(2.0 * FS) as usize / BLOCK {
            let mut l = vec![0.0f32; BLOCK];
            let mut r = vec![0.0f32; BLOCK];
            let status = {
                let mut refs: Vec<&mut [f32]> = vec![&mut l, &mut r];
                plugin.process_block(&mut refs, Some(120.0))
            };
            if let (None, ProcessStatus::Tail(n)) = (told, status) {
                told = Some(((k + 1) * BLOCK, n));
            }
            if let Some(i) = l.iter().rposition(|x| x.abs() > 100.0 * SNAP_LEVEL) {
                last_heard = Some(k * BLOCK + i);
            }
        }
        let (from, n) = told.expect("the host was never told a tail");
        let last = last_heard.expect("the reversed click never came out");
        assert!(
            from + n as usize > last,
            "told a tail of {:.3} s at {:.3} s, but the click was still coming out at {:.3} s",
            n as f32 / FS,
            from as f32 / FS,
            last as f32 / FS
        );
    }

    /// A finite tail ends: the status returns to `Normal` rather than `Tail` once the loop has
    /// snapped to exact zero.
    #[test]
    fn a_finite_tail_ends_and_the_status_says_so() {
        let mut plugin = plugin(1);
        set_float(&plugin, "mix", 1.0);
        set_float(&plugin, "feedback", 0.3);
        plugin.settle();
        run(&mut plugin, &tone((0.2 * FS) as usize, 220.0, 0.9), 256);

        let mut status = ProcessStatus::Normal;
        for _ in 0..400 {
            let mut l = vec![0.0f32; 256];
            let mut r = vec![0.0f32; 256];
            let mut refs: Vec<&mut [f32]> = vec![&mut l, &mut r];
            status = plugin.process_block(&mut refs, Some(120.0));
        }
        assert!(
            matches!(status, ProcessStatus::Normal),
            "the plugin is still claiming a tail: {status:?}"
        );
        assert!(plugin.core.is_parked(), "the core did not stop");
    }

    /// The two synced positions differ **by measurement**, not by preference: `Glide` moves the
    /// delay continuously into a new tempo and `Snap` arrives at once.
    #[test]
    fn glide_glides_and_snap_snaps() {
        for (mode, expect_gradual) in [(ChangeChoice::Glide, true), (ChangeChoice::Snap, false)] {
            let mut plugin = plugin(1);
            sync(&plugin, mode);
            plugin.settle();

            // One block at 120 bpm settles the time, then the tempo halves.
            run(&mut plugin, &vec![0.0; 4096], 256);
            let before = plugin.time_s;

            let mut l = vec![0.0f32; 64];
            let mut r = vec![0.0f32; 64];
            let mut refs: Vec<&mut [f32]> = vec![&mut l, &mut r];
            plugin.process_block(&mut refs, Some(60.0));
            let after_one_block = plugin.time_s;
            let target = plugin.target_time(Some(60.0));

            if expect_gradual {
                assert!(
                    (after_one_block - target).abs() > 0.1 * (target - before).abs(),
                    "Glide arrived at once: {before} to {after_one_block}, target {target}"
                );
            } else {
                assert!(
                    (after_one_block - target).abs() < 1e-6,
                    "Snap did not arrive at once: {after_one_block} against {target}"
                );
            }
        }
    }

    /// **The Time knob is live when synced**, which is the defect the owner reported: *"when time
    /// is set to snap it is hardcoded to 1/8"*. Turning it has to walk the delay through the
    /// subdivisions the tempo and the fitted chip can hold, and stop at the ends rather than lie.
    #[test]
    fn the_time_knob_walks_the_subdivisions_when_synced() {
        let plugin = plugin(1);
        sync(&plugin, ChangeChoice::Snap);

        let at = |plugin: &MxmBucketDelay, plain: f32| {
            set_float(plugin, "time", plain);
            plugin.target_time(Some(120.0))
        };
        let shortest = at(&plugin, params::MIN_TIME_S);
        let longest = at(&plugin, params::MAX_TIME_S);
        assert!(
            longest > shortest * 4.0,
            "the knob barely moved the delay: {shortest} to {longest}"
        );

        // Monotonic, and it visits more than a couple of places.
        let mut seen: Vec<f32> = Vec::new();
        for n in 0..=40 {
            let plain = params::time_range().unnormalize(n as f32 / 40.0);
            let t = at(&plugin, plain);
            if let Some(&last) = seen.last() {
                assert!(t >= last, "turning up shortened the delay: {last} then {t}");
            }
            if seen.last() != Some(&t) {
                seen.push(t);
            }
        }
        assert!(
            seen.len() >= 8,
            "the knob only reached {} delays across its travel",
            seen.len()
        );

        // And the chip is a real limit: the 1024-stage part cannot hold what the 8192 can.
        unsafe {
            let v = plugin
                .params
                .line
                .preview_normalized(params::LineChoice::S1024);
            let _ = plugin.params.line._internal_set_normalized_value(v);
        }
        let short_chip = at(&plugin, params::MAX_TIME_S);
        assert!(
            short_chip < longest,
            "the short chip claimed the long chip's delay: {short_chip} against {longest}"
        );
    }

    /// Without a transport there is still a delay: a host that reports no tempo gets the knob
    /// rather than silence.
    #[test]
    fn a_host_with_no_tempo_falls_back_to_the_knob() {
        let plugin = plugin(1);
        sync(&plugin, ChangeChoice::Glide);
        assert_eq!(plugin.target_time(None), plugin.params.time.value());
    }

    /// **Change is inert while Time is free**: a snapped change is a synced tail's law, so a free
    /// knob turned under Snap still glides, exactly as the build before the split did.
    #[test]
    fn change_is_inert_while_time_is_free() {
        let mut plugin = plugin(1);
        unsafe {
            let v = plugin.params.change.preview_normalized(ChangeChoice::Snap);
            let _ = plugin.params.change._internal_set_normalized_value(v);
        }
        set_float(&plugin, "time", 0.1);
        plugin.settle();
        run(&mut plugin, &vec![0.0; 4096], 256);
        set_float(&plugin, "time", 1.0);
        let mut l = vec![0.0f32; 64];
        let mut r = vec![0.0f32; 64];
        let mut refs: Vec<&mut [f32]> = vec![&mut l, &mut r];
        plugin.process_block(&mut refs, Some(120.0));
        assert!(
            plugin.time_s < 0.9,
            "a free knob under Snap arrived at once: {}",
            plugin.time_s
        );
    }

    /// **The wobble's Rate sync picks a division and is inert without a tempo**
    /// (`plans/plan-tempo-sync-controls.md`): off, or with no tempo, the knob's own hertz stand; on at
    /// 120 bpm the ends are the LFO ladder's ends, the top the fastest.
    #[test]
    fn rate_sync_picks_a_division_and_is_inert_without_a_tempo() {
        use mxm_tempo::Division;
        let plugin = plugin(1);
        let set_rate = |normalized: f32| unsafe {
            let _ = plugin
                .params
                .rate
                ._internal_set_normalized_value(normalized);
        };

        set_rate(1.0);
        assert_eq!(
            plugin.synced_rate(Some(120.0)),
            None,
            "off is the free rate"
        );
        unsafe {
            let _ = plugin.params.rate_sync._internal_set_normalized_value(1.0);
        }
        assert_eq!(plugin.synced_rate(None), None, "no tempo is the free rate");

        let top = plugin.synced_rate(Some(120.0)).expect("synced at a tempo");
        set_rate(0.0);
        let bottom = plugin.synced_rate(Some(120.0)).expect("synced at a tempo");
        let (lo, hi) = (
            f64::from(plugin.params.rate.preview_plain(0.0)),
            f64::from(plugin.params.rate.preview_plain(1.0)),
        );
        assert!(
            top > bottom,
            "the top of a rate is the fastest: {bottom} to {top}"
        );
        let fastest = Division::ThirtySecond.hz(120.0).clamp(lo, hi) as f32;
        let slowest = Division::FourBars.hz(120.0).clamp(lo, hi) as f32;
        assert!((top - fastest).abs() < 1e-4, "{top} against {fastest}");
        assert!(
            (bottom - slowest).abs() < 1e-4,
            "{bottom} against {slowest}"
        );
    }

    /// **A loaded preset's baseline splits with the Sync**, so an unchanged preset in an old project
    /// stays clean: Free (0) is off, Glide (½) is on with Glide, Snap (1) is on with Snap.
    #[test]
    fn an_old_preset_baseline_splits_with_the_sync() {
        use nice_plug::params::persist::{deserialize_field, serialize_field};
        for (old, sync, change) in [(0.0, 0.0, 0.0), (0.5, 1.0, 0.0), (1.0, 1.0, 1.0)] {
            let identity = mxm_preset::PresetIdentity {
                loaded: Some(mxm_preset::LoadedPreset {
                    name: "Old".to_owned(),
                    origin: mxm_preset::Origin::Factory,
                }),
                baseline: [("sync".to_owned(), old)].into_iter().collect(),
                ..mxm_preset::PresetIdentity::default()
            };
            let mut fields: std::collections::BTreeMap<String, String> =
                [("preset".to_owned(), serialize_field(&identity).unwrap())]
                    .into_iter()
                    .collect();
            migrate_sync_baseline(&mut fields);
            let split: mxm_preset::PresetIdentity = deserialize_field(&fields["preset"]).unwrap();
            assert_eq!(split.baseline["sync"], sync, "{old}");
            assert_eq!(split.baseline["change"], change, "{old}");
        }
    }

    /// **An old project's three-way Sync reads into the split.** Free is off; Glide and Snap are on,
    /// with that change; a state already split is left alone.
    #[test]
    fn an_old_three_way_sync_migrates_into_sync_and_change() {
        use std::collections::BTreeMap;
        for (old, on, change) in [
            ("free", false, "glide"),
            ("glide", true, "glide"),
            ("snap", true, "snap"),
        ] {
            let mut params = BTreeMap::new();
            params.insert("sync".to_owned(), ParamValue::String(old.to_owned()));
            migrate_sync(&mut params);
            assert!(
                matches!(params.get("sync"), Some(ParamValue::Bool(b)) if *b == on),
                "{old}"
            );
            assert!(
                matches!(params.get("change"), Some(ParamValue::String(c)) if c == change),
                "{old}"
            );
        }

        let mut split = BTreeMap::new();
        split.insert("sync".to_owned(), ParamValue::Bool(true));
        split.insert("change".to_owned(), ParamValue::String("snap".to_owned()));
        let before = format!("{split:?}");
        migrate_sync(&mut split);
        assert_eq!(format!("{split:?}"), before);

        // A value the old enum never had is not the old enum: left as it is, not read as Sync on.
        let mut unknown = BTreeMap::new();
        unknown.insert("sync".to_owned(), ParamValue::String("invalid".to_owned()));
        let before = format!("{unknown:?}");
        assert!(!migrate_sync(&mut unknown));
        assert_eq!(format!("{unknown:?}"), before);
    }
}

/// **A reset re-resolves the tempo syncs**: a sync turned off while the host held the effect
/// unprocessed does not seed the reset from the previous division, and one still on stays on it.
#[cfg(test)]
mod reset_resolves_the_syncs {
    use super::*;
    use nice_plug::params::InternalParamMut;

    #[test]
    fn a_reset_re_resolves_the_tempo_syncs() {
        let mut plugin = MxmBucketDelay {
            synced_rate_hz: Some(9.0),
            ..Default::default()
        };
        plugin.reset();
        assert_eq!(plugin.synced_rate_hz, None, "sync off: the knob");
        plugin.telemetry.publish_tempo(Some(120.0));
        unsafe {
            let _ = plugin.params.rate_sync._internal_set_normalized_value(1.0);
            let _ = plugin.params.sync._internal_set_normalized_value(1.0);
        }
        plugin.reset();
        assert_eq!(plugin.synced_rate_hz, plugin.synced_rate(Some(120.0)));
        assert_eq!(
            plugin.time_s,
            plugin.target_time(Some(120.0)),
            "Time starts synced"
        );
    }

    /// **Reactivation forgets the old tempo**: nice-plug resets right after activating, and that
    /// reset must not resolve from the tempo the host reported before it was deactivated — the first
    /// callback's tempo is the first one used.
    #[test]
    fn reactivation_forgets_the_previous_tempo() {
        let mut plugin = MxmBucketDelay::default();
        plugin.telemetry.publish_tempo(Some(120.0));
        unsafe {
            let _ = plugin.params.rate_sync._internal_set_normalized_value(1.0);
        }
        plugin.prepare(48_000.0, 1);
        plugin.reset();
        assert_eq!(plugin.synced_rate_hz, None);
    }
}

/// What a player reads — on hover in the editor, and in a host's plugin browser — speaks to the
/// player about the sound, never about the machine or the code (`mxm_plugin_test::hover_text`).
#[cfg(test)]
mod speaks_to_the_player {
    #[test]
    fn hover_text() {
        mxm_plugin_test::hover_text::speaks_to_the_player(env!("CARGO_MANIFEST_DIR"));
    }

    #[test]
    fn host_description() {
        mxm_plugin_test::hover_text::host_description_speaks_to_the_player(env!(
            "CARGO_MANIFEST_DIR"
        ));
    }
}
