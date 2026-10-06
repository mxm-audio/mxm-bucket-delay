//! Presets: this plugin's factory set, and what the collection's preset crate needs of it.
//!
//! The format, the library on disk, favourites, the loaded identity and the app-bar controls are
//! `mxm-preset`'s. What is left here is what only this plugin knows: its id, its parameters, and
//! its sounds.
//!
//! # Why this one needs presets more than either sibling
//!
//! `plans/plan-mxm-fx-collection.md` §2 ships the preset system **when a meaningful parameter
//! exists beyond level**. Here there are twenty-one, and six of them are a *shape* rather than an
//! amount: the tap constellation is the most interesting thing the device family can do, and
//! arriving at it by moving six faders is not something anyone will do by accident. The init patch
//! is deliberately a plain single echo, so the constellation is something the presets **show you**.
//!
//! # The ladder is a preset, not a hidden gain
//!
//! Panasonic's reference reverberation circuit mixes the six taps through a 100–150 kΩ resistor
//! ladder — a gentle 3.5 dB tilt across the set, not a decay envelope. With the faders exposed the
//! faders *are* that ladder, so it lives in `Reverberation` rather than behind the controls where
//! nobody could see or change it. [`crate::params::LADDER`] is the computed shape.

use std::sync::RwLock;

pub use mxm_preset::{
    Category, Entry, INIT_NAME, Library, Loaded, Origin, Preset, PresetIdentity, Refused, Value,
    factory, loaded, mark_loaded, mark_none, read_favourites, snapshot, write_favourites,
};

use crate::params::MxmBucketDelayParams;

/// **The tempo syncs this plugin gained on 2026-09-25** (`plans/plan-tempo-sync-controls.md`). A
/// preset file written before them was written unsynced, so each loads off rather than keeping the
/// instance's sync, and without reporting a missing control.
/// The switches an older project restores Off (`filter_state`): Rate sync. Sync and Change are
/// split from the old three-way Sync instead (`migrate_sync`).
pub(crate) const RESTORED_OFF: &[&str] = &["ratesync"];

pub(crate) const TEMPO_SYNC_IDS: &[&str] = &["change", "ratesync"];

impl mxm_preset::Instrument for MxmBucketDelayParams {
    fn clap_id(&self) -> &'static str {
        crate::CLAP_ID
    }

    /// In declaration order, from the one list the editor draws from.
    fn parameters(&self) -> Vec<(&'static str, &dyn mxm_preset::ErasedParam)> {
        crate::editor::sections::all_parameters(self)
            .into_iter()
            .map(|bound| (bound.id, bound.param))
            .collect()
    }

    fn identity(&self) -> &RwLock<PresetIdentity> {
        &self.preset
    }

    fn factory_files(&self) -> &'static [(&'static str, &'static str)] {
        FACTORY_FILES
    }

    fn default_missing_legacy_parameter(&self, id: &str) -> bool {
        TEMPO_SYNC_IDS.contains(&id)
    }
}

/// The factory set, compiled in. Init is not one of them: Init is the plain echo the plugin opens
/// at, and each of these is a thing the device can do that the plain echo does not show.
pub const FACTORY_FILES: &[(&str, &str)] = &[
    ("Slapback", include_str!("../presets/slapback.json")),
    ("Tape slap", include_str!("../presets/tape-slap.json")),
    ("Long echo", include_str!("../presets/long-echo.json")),
    ("Dub", include_str!("../presets/dub.json")),
    ("Dark repeats", include_str!("../presets/dark-repeats.json")),
    (
        "Bright repeats",
        include_str!("../presets/bright-repeats.json"),
    ),
    ("Runaway", include_str!("../presets/runaway.json")),
    ("Quarter note", include_str!("../presets/quarter-note.json")),
    (
        "Dotted eighth",
        include_str!("../presets/dotted-eighth.json"),
    ),
    ("Triplet echo", include_str!("../presets/triplet-echo.json")),
    (
        "Sixteenth stutter",
        include_str!("../presets/sixteenth-stutter.json"),
    ),
    ("Half note", include_str!("../presets/half-note.json")),
    (
        "Reverberation",
        include_str!("../presets/reverberation.json"),
    ),
    ("Chamber", include_str!("../presets/chamber.json")),
    ("Hall", include_str!("../presets/hall.json")),
    ("Bloom", include_str!("../presets/bloom.json")),
    ("Dark room", include_str!("../presets/dark-room.json")),
    ("Cavern", include_str!("../presets/cavern.json")),
    (
        "Close ambience",
        include_str!("../presets/close-ambience.json"),
    ),
    (
        "Drifting wash",
        include_str!("../presets/drifting-wash.json"),
    ),
    ("Wide bloom", include_str!("../presets/wide-bloom.json")),
    (
        "Tight cluster",
        include_str!("../presets/tight-cluster.json"),
    ),
    ("Two tap", include_str!("../presets/two-tap.json")),
    ("Gallop", include_str!("../presets/gallop.json")),
    ("Scatter", include_str!("../presets/scatter.json")),
    (
        "Three against two",
        include_str!("../presets/three-against-two.json"),
    ),
    ("Backbeat", include_str!("../presets/backbeat.json")),
    ("Wide scatter", include_str!("../presets/wide-scatter.json")),
    ("Comb rhythm", include_str!("../presets/comb-rhythm.json")),
    ("Chorus", include_str!("../presets/chorus.json")),
    ("Vibrato", include_str!("../presets/vibrato.json")),
    ("Deep flange", include_str!("../presets/deep-flange.json")),
    ("Seasick", include_str!("../presets/seasick.json")),
    ("Warble tape", include_str!("../presets/warble-tape.json")),
    ("Slow drift", include_str!("../presets/slow-drift.json")),
    (
        "Wow and flutter",
        include_str!("../presets/wow-and-flutter.json"),
    ),
    (
        "Detune double",
        include_str!("../presets/detune-double.json"),
    ),
    (
        "Reverse swell",
        include_str!("../presets/reverse-swell.json"),
    ),
    ("Reverse stab", include_str!("../presets/reverse-stab.json")),
    (
        "Backwards wash",
        include_str!("../presets/backwards-wash.json"),
    ),
    (
        "Reverse rhythm",
        include_str!("../presets/reverse-rhythm.json"),
    ),
    ("Below spec", include_str!("../presets/below-spec.json")),
    ("Gritty", include_str!("../presets/gritty.json")),
    ("Starved", include_str!("../presets/starved.json")),
    ("Lo-fi short", include_str!("../presets/lo-fi-short.json")),
    ("Singing line", include_str!("../presets/singing-line.json")),
    ("Noise floor", include_str!("../presets/noise-floor.json")),
    ("Ping-pong", include_str!("../presets/ping-pong.json")),
    (
        "Wide ping-pong",
        include_str!("../presets/wide-ping-pong.json"),
    ),
    ("Mono tight", include_str!("../presets/mono-tight.json")),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// **A project saved before the tempo syncs restores them Off** (`mxm_preset::add_switches_off`),
    /// whatever this instance had.
    #[test]
    fn an_older_state_restores_the_tempo_syncs_off() {
        use nice_plug::prelude::Plugin as _;
        let mut state = nice_plug::prelude::PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        };
        crate::MxmBucketDelay::filter_state(&mut state);
        for id in RESTORED_OFF {
            assert!(
                matches!(
                    state.params.get(*id),
                    Some(nice_plug::plugin::ParamValue::Bool(false))
                ),
                "{{id}} was not restored off"
            );
        }
    }

    /// **A preset saved before the tempo syncs loads them off, and cleanly** ([`TEMPO_SYNC_IDS`]).
    #[test]
    fn a_preset_from_before_the_tempo_syncs_loads_them_off() {
        let params = crate::params::MxmBucketDelayParams::default();
        let mut old = mxm_preset::Preset::init(&params);
        for id in TEMPO_SYNC_IDS {
            old.params.remove(*id);
        }
        let (writes, problems) = old.resolve(&params);
        assert!(problems.is_empty(), "{{problems:?}}");
        for id in TEMPO_SYNC_IDS {
            assert!(
                writes.iter().any(|(w, _, v)| w == id && *v == 0.0),
                "{{id}} was not written off"
            );
        }
    }

    use nice_plug::params::Param;

    fn params() -> MxmBucketDelayParams {
        MxmBucketDelayParams::default()
    }

    /// Prints every parameter as eleven `normalised=formatted` steps, for preset design.
    ///
    /// ```text
    /// cargo test -p mxm-bucket-delay --lib the_mapping_table -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "prints what each normalised value means, for preset design"]
    fn the_mapping_table() {
        let params = params();
        for bound in crate::editor::sections::all_parameters(&params) {
            let steps: Vec<String> = (0..=10)
                .map(|i| {
                    let v = i as f32 / 10.0;
                    format!("{v:.1}={}", bound.param.format(v))
                })
                .collect();
            eprintln!("{:<10} {}", bound.id, steps.join("  "));
        }
    }

    /// Every factory file parses, names this plugin, and sets only parameters that exist.
    #[test]
    fn every_factory_preset_is_loadable() {
        let params = params();
        let presets = factory(&params);
        // `factory` puts Init at the head of the list, so the count is the files plus one. A file
        // that failed to parse is silently dropped, which is why this is a count and not a glance.
        assert_eq!(
            presets.len(),
            FACTORY_FILES.len() + 1,
            "a factory file failed to parse"
        );
        for (name, _) in FACTORY_FILES {
            assert!(
                presets.iter().any(|p| p.name == *name),
                "{name} is not in the loaded set"
            );
        }
        for preset in presets {
            assert_eq!(preset.plugin, crate::CLAP_ID);
            assert!(!preset.name.is_empty());
        }
    }

    /// **Fifty, and Init.** The owner's ask of 2026-09-06. The count is asserted because the bank
    /// is generated by `examples/build_presets.rs` and a table entry lost in an edit would
    /// otherwise just be a preset nobody notices is missing.
    #[test]
    fn the_factory_bank_is_fifty_presets_and_init() {
        assert_eq!(FACTORY_FILES.len(), 50, "the bank changed size");
        let params = params();
        assert_eq!(factory(&params).len(), 51, "fifty presets and Init");
    }

    /// No two presets share a name. A duplicate is two indistinguishable rows in the browser, and
    /// with fifty of them nobody would spot it by looking.
    #[test]
    fn every_preset_name_is_its_own() {
        let mut seen = std::collections::HashSet::new();
        for (name, _) in FACTORY_FILES {
            assert!(
                seen.insert(*name),
                "{name} appears twice in the factory bank"
            );
        }
    }

    /// **The files mean what their names say.** A preset stores *normalised* values, and this
    /// plugin has a skewed Time range and six enums, so a number that is a hundred milliseconds out
    /// looks exactly like a number that is right. Each claim below is the sound's own description,
    /// checked by unnormalising through the parameter itself.
    ///
    /// **Two kinds of claim, because `Time` means two things.** Free-running, it is seconds.
    /// Synced, it is a *position* that selects a subdivision — so a synced preset is checked
    /// against the subdivision it picks, which is what its name promises, and not against the
    /// seconds it happens to store.
    #[test]
    fn every_factory_preset_lands_where_its_description_says() {
        let p = params();
        let by_name: std::collections::HashMap<String, Preset> = factory(&p)
            .into_iter()
            .map(|preset| (preset.name.clone(), preset))
            .collect();

        let seconds = |preset: &Preset| -> f32 {
            p.time
                .preview_plain(preset.params.get("time").expect("time is set").v)
        };
        let choice = |preset: &Preset, id: &str| -> f32 { preset.params.get(id).expect(id).v };

        // The free-running ones, in seconds.
        for (name, want) in [
            ("Slapback", 0.095),
            ("Long echo", 0.62),
            ("Dub", 0.42),
            ("Reverberation", 0.30),
            ("Reverse swell", 0.75),
            ("Cavern", 0.90),
            ("Below spec", 1.80),
        ] {
            let got = seconds(&by_name[name]);
            assert!(
                (got - want).abs() < 0.005,
                "{name} is at {got} s where it says {want} s"
            );
        }
        // The chorus is a *very* short delay, which is how this chip family makes one.
        assert!(seconds(&by_name["Chorus"]) < 0.03);
        assert!(seconds(&by_name["Vibrato"]) < 0.03);

        // The synced ones, by the subdivision the knob selects. Checked on the preset's own line,
        // because a chip that cannot hold a division does not get offered it.
        for (name, want) in [
            ("Quarter note", "1/4"),
            ("Dotted eighth", "1/8."),
            ("Triplet echo", "1/8T"),
            ("Sixteenth stutter", "1/16"),
            ("Half note", "1/2"),
            ("Ping-pong", "1/8"),
            ("Wide ping-pong", "1/4"),
            ("Backbeat", "1/4"),
            ("Three against two", "1/8T"),
            ("Reverse rhythm", "1/8"),
        ] {
            let preset = &by_name[name];
            assert_ne!(
                choice(preset, "sync"),
                0.0,
                "{name} names a subdivision but is not synced"
            );
            let line = p.line.preview_plain(choice(preset, "line")).line();
            let (lo, hi) = crate::params::time_bounds(line);
            let got = crate::params::TIME_SYNC.division(choice(preset, "time"), 120.0, lo, hi);
            assert_eq!(
                got.label(),
                want,
                "{name} selects {} at 120 bpm on {line:?}, not {want}",
                got.label()
            );
        }

        // And the enums, which are the other half of what a hand-typed file gets wrong.
        assert_eq!(
            choice(&by_name["Ping-pong"], "routing"),
            1.0,
            "ping-pong is the last routing"
        );
        assert_eq!(choice(&by_name["Reverse swell"], "reverse"), 1.0);
        assert_eq!(choice(&by_name["Backwards wash"], "reverse"), 1.0);
        assert_eq!(
            choice(&by_name["Chorus"], "feedback"),
            0.0,
            "the chorus has no loop at all"
        );
        assert_eq!(
            choice(&by_name["Reverberation"], "return"),
            0.0,
            "the wash is fed by the mix"
        );
    }

    /// The reverberation preset is the six-tap constellation, which is the one shape the init patch
    /// deliberately does not open. If it ever stops opening all six, the product ships without ever
    /// showing the thing it was built for.
    #[test]
    fn the_reverberation_preset_opens_all_six_taps() {
        let params = params();
        let preset = factory(&params)
            .into_iter()
            .find(|p| p.name == "Reverberation")
            .expect("the reverberation preset is in the factory set");
        for (index, id) in ["tap1", "tap2", "tap3", "tap4", "tap5", "tap6"]
            .into_iter()
            .enumerate()
        {
            let value = preset
                .params
                .get(id)
                .unwrap_or_else(|| panic!("{id} is not in the preset"));
            assert!(value.v > 0.5, "{id} is only at {}", value.v);
            // And it is *the ladder*, not six faders at the top: the reference circuit's own
            // 100-150 kOhm weighting, which is a 3.5 dB tilt and not a decay.
            assert!(
                (value.v - crate::params::LADDER[index]).abs() < 1e-3,
                "{id} is at {} where the ladder says {}",
                value.v,
                crate::params::LADDER[index]
            );
        }
    }
}
