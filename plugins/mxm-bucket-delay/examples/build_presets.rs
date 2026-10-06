//! The factory bank: fifty presets, written from the table below.
//!
//! ```bash
//! cargo run -p mxm-bucket-delay --release --example build_presets
//! ```
//!
//! **This file is the source, `presets/*.json` is the output.** Fifty hand-typed JSON files storing
//! *normalised* values against a skewed Time range and six enums is fifty chances to write a number
//! that looks right and is a hundred milliseconds wrong — the exact defect
//! `every_factory_preset_lands_where_its_description_says` exists to catch. So the table says what
//! each patch *is*, in the same words the panel uses, and the plugin's own parsers turn that into
//! numbers:
//!
//! ```text
//! ("time", "420 ms")   ->  the Time parameter's own string_to_value
//! ("line", "8192")     ->  the enum's own variant name
//! ("bias", "+45 %")    ->  "Trimmed" at the centre, as the panel writes it
//! ```
//!
//! A string the parameter refuses is a panic naming the id and the text, so a typo cannot become a
//! preset. What a preset does not mention keeps **Init**'s value, which is why each entry below
//! reads as a difference from the plain echo rather than as nineteen numbers.
//!
//! # Regenerating overwrites the files
//!
//! Hand-edit a preset and the next run of this will discard it. That is the trade for the table
//! being the record: change a patch *here*, run this, then run `bucket_delay_preset_audit` to hear what the
//! numbers became. `plugins/mxm-bucket-delay/AGENTS.md` records the rule.

use mxm_bucket_delay::params::{MxmBucketDelayParams, TIME_SYNC, time_range};
use mxm_bucket_delay::preset::{Category, Preset};
use nice_plug::prelude::*;
use std::path::PathBuf;

/// A host that applies what it is asked to.
struct ApplyingHost;

impl nice_plug::context::gui::GuiContextInner for ApplyingHost {
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

/// One patch: a file name, a display name, and what it changes about Init.
struct Patch {
    slug: &'static str,
    name: &'static str,
    /// `(parameter id, the value written the way the panel writes it)`.
    set: Vec<(&'static str, String)>,
}

fn p(slug: &'static str, name: &'static str, set: &[(&'static str, &str)]) -> Patch {
    Patch {
        slug,
        name,
        set: set.iter().map(|(id, v)| (*id, (*v).to_owned())).collect(),
    }
}

/// The **Time value that selects a subdivision** when `Sync` is following the host.
///
/// Synced, the knob is a selector: its position across the whole travel picks one of the eleven
/// subdivisions on `TIME_SYNC`'s ladder. So a synced patch has to store the position, and this is
/// the only honest way to write one down — `at("1/8.")` rather than a number nobody could check.
/// The `text` such a preset carries is that position in *seconds*, which is what the knob would mean
/// with `Sync` off, and that is not a mistake: it is the value stored.
fn at(division: &str) -> String {
    let division = mxm_tempo::Division::parse(division)
        .filter(|&d| TIME_SYNC.span.contains(d))
        .unwrap_or_else(|| panic!("{division} is not one of this plugin's subdivisions"));
    let seconds = time_range().unnormalize(TIME_SYNC.position(division));
    if seconds < 1.0 {
        format!("{:.0} ms", seconds * 1000.0)
    } else {
        format!("{seconds:.2} s")
    }
}

/// The reference reverberation circuit's own ladder, as six strings.
///
/// **Two decimals, where the panel writes none.** The ladder is a resistor ratio — 0.909, not 91 %
/// — and `{:.0} %` round-trips it to 0.91, which is exactly the tolerance
/// `the_reverberation_preset_opens_all_six_taps` holds the file to. The parser takes any float, so
/// the precision costs nothing; the `text` the preset carries is still the parameter's own `91 %`.
fn ladder() -> Vec<(&'static str, String)> {
    ["tap1", "tap2", "tap3", "tap4", "tap5", "tap6"]
        .into_iter()
        .zip(mxm_bucket_delay::params::LADDER)
        .map(|(id, g)| (id, format!("{:.2} %", g * 100.0)))
        .collect()
}

/// Six taps wide open, which is the ladder's flat cousin: every return at one level.
fn six_open() -> Vec<(&'static str, String)> {
    ["tap1", "tap2", "tap3", "tap4", "tap5", "tap6"]
        .into_iter()
        .map(|id| (id, "100 %".to_owned()))
        .collect()
}

fn with(
    base: Vec<(&'static str, String)>,
    more: &[(&'static str, &str)],
) -> Vec<(&'static str, String)> {
    let mut out = base;
    out.extend(more.iter().map(|(id, v)| (*id, (*v).to_owned())));
    out
}

// **`vec![]` would be one 700-line literal.** Clippy is right that a `Vec::new` followed by pushes
// is usually a literal written the long way; here the pushes are grouped by what the patches *are*,
// with a paragraph of prose over each family, and that grouping is the point of the file.
#[allow(clippy::vec_init_then_push)]
fn catalogue() -> Vec<Patch> {
    let mut all = Vec::new();

    // ---- Echoes: one tap, the loop fed by the last return. What the device is before anything
    // is opened up, at every length the four chips reach.
    all.push(p(
        "slapback",
        "Slapback",
        &[
            ("line", "1024"),
            ("time", "95 ms"),
            ("feedback", "12 %"),
            ("mix", "40 %"),
            ("filter", "Tracking"),
            ("routing", "Mono"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "tape-slap",
        "Tape slap",
        &[
            ("time", "120 ms"),
            ("feedback", "28 %"),
            ("mix", "42 %"),
            ("bias", "+30 %"),
            ("wobble", "14 %"),
            ("rate", "0.800 Hz"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "long-echo",
        "Long echo",
        &[
            ("line", "8192"),
            ("time", "620 ms"),
            ("feedback", "55 %"),
            ("mix", "35 %"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "dub",
        "Dub",
        &[
            ("time", "420 ms"),
            ("feedback", "78 %"),
            ("mix", "55 %"),
            ("tap4", "35 %"),
            ("bias", "+45 %"),
            ("wobble", "18 %"),
            ("rate", "0.350 Hz"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "dark-repeats",
        "Dark repeats",
        &[
            ("line", "8192"),
            ("time", "780 ms"),
            ("feedback", "62 %"),
            ("mix", "38 %"),
            ("filter", "Fixed"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "bright-repeats",
        "Bright repeats",
        &[
            ("line", "1024"),
            ("time", "320 ms"),
            ("feedback", "58 %"),
            ("mix", "40 %"),
            ("filter", "Tracking"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "runaway",
        "Runaway",
        &[
            ("time", "380 ms"),
            ("feedback", "86 %"),
            ("mix", "45 %"),
            ("return", "Tail"),
        ],
    ));

    // ---- Synced echoes. The Time knob is the subdivision selector here, so every one of these
    // stores a knob *position* and `at()` is what writes it.
    all.push(Patch {
        slug: "quarter-note",
        name: "Quarter note",
        set: vec![
            ("sync", "On".into()),
            ("change", "Snap".into()),
            ("time", at("1/4")),
            ("feedback", "58 %".into()),
            ("mix", "42 %".into()),
            ("return", "Tail".into()),
        ],
    });
    all.push(Patch {
        slug: "dotted-eighth",
        name: "Dotted eighth",
        set: vec![
            ("line", "3328".into()),
            ("sync", "On".into()),
            ("time", at("1/8.")),
            ("feedback", "62 %".into()),
            ("mix", "42 %".into()),
            ("return", "Tail".into()),
        ],
    });
    all.push(Patch {
        slug: "triplet-echo",
        name: "Triplet echo",
        set: vec![
            ("sync", "On".into()),
            ("change", "Snap".into()),
            ("time", at("1/8T")),
            ("feedback", "55 %".into()),
            ("mix", "40 %".into()),
            ("return", "Tail".into()),
        ],
    });
    all.push(Patch {
        slug: "sixteenth-stutter",
        name: "Sixteenth stutter",
        set: vec![
            ("line", "1024".into()),
            ("sync", "On".into()),
            ("change", "Snap".into()),
            ("time", at("1/16")),
            ("feedback", "65 %".into()),
            ("mix", "45 %".into()),
            ("filter", "Tracking".into()),
            ("return", "Tail".into()),
        ],
    });
    all.push(Patch {
        slug: "half-note",
        name: "Half note",
        set: vec![
            ("line", "8192".into()),
            ("sync", "On".into()),
            ("change", "Snap".into()),
            ("time", at("1/2")),
            ("feedback", "60 %".into()),
            ("mix", "40 %".into()),
            ("return", "Tail".into()),
        ],
    });

    // ---- Washes: all six taps into the loop. The thing the six-tap part was sold to do, and the
    // one shape the init patch deliberately does not open.
    all.push(Patch {
        slug: "reverberation",
        name: "Reverberation",
        set: with(
            ladder(),
            &[
                ("line", "3328"),
                ("time", "300 ms"),
                ("feedback", "78 %"),
                ("mix", "45 %"),
                ("return", "Mix"),
            ],
        ),
    });
    all.push(Patch {
        slug: "chamber",
        name: "Chamber",
        set: with(
            ladder(),
            &[
                ("line", "3328"),
                ("time", "170 ms"),
                ("feedback", "76 %"),
                ("mix", "40 %"),
                ("filter", "Tracking"),
                ("return", "Mix"),
            ],
        ),
    });
    all.push(Patch {
        slug: "hall",
        name: "Hall",
        set: with(
            six_open(),
            &[
                ("line", "8192"),
                ("time", "520 ms"),
                ("feedback", "84 %"),
                ("mix", "48 %"),
                ("return", "Mix"),
            ],
        ),
    });
    all.push(Patch {
        slug: "bloom",
        name: "Bloom",
        set: vec![
            ("tap1", "30 %".into()),
            ("tap2", "45 %".into()),
            ("tap3", "60 %".into()),
            ("tap4", "75 %".into()),
            ("tap5", "90 %".into()),
            ("tap6", "100 %".into()),
            ("time", "360 ms".into()),
            ("feedback", "80 %".into()),
            ("mix", "50 %".into()),
            ("return", "Mix".into()),
        ],
    });
    all.push(Patch {
        slug: "dark-room",
        name: "Dark room",
        set: with(
            ladder(),
            &[
                ("line", "8192"),
                ("time", "240 ms"),
                ("feedback", "74 %"),
                ("mix", "42 %"),
                ("filter", "Fixed"),
                ("return", "Mix"),
            ],
        ),
    });
    all.push(Patch {
        slug: "cavern",
        name: "Cavern",
        set: with(
            six_open(),
            &[
                ("line", "8192"),
                ("time", "900 ms"),
                ("feedback", "86 %"),
                ("mix", "55 %"),
                ("return", "Mix"),
            ],
        ),
    });
    all.push(Patch {
        slug: "close-ambience",
        name: "Close ambience",
        set: with(
            ladder(),
            &[
                ("line", "1024"),
                ("time", "55 ms"),
                ("feedback", "70 %"),
                ("mix", "35 %"),
                ("filter", "Tracking"),
                ("return", "Mix"),
            ],
        ),
    });
    all.push(Patch {
        slug: "drifting-wash",
        name: "Drifting wash",
        set: with(
            six_open(),
            &[
                ("time", "300 ms"),
                ("feedback", "78 %"),
                ("mix", "50 %"),
                ("wobble", "35 %"),
                ("rate", "0.250 Hz"),
                ("return", "Mix"),
            ],
        ),
    });
    all.push(Patch {
        slug: "wide-bloom",
        name: "Wide bloom",
        set: with(
            ladder(),
            &[
                ("line", "3328"),
                ("time", "400 ms"),
                ("feedback", "80 %"),
                ("mix", "42 %"),
                ("spread", "1.60"),
                ("return", "Mix"),
            ],
        ),
    });
    all.push(Patch {
        slug: "tight-cluster",
        name: "Tight cluster",
        set: with(
            six_open(),
            &[
                ("line", "3328"),
                ("time", "200 ms"),
                ("feedback", "76 %"),
                ("mix", "45 %"),
                ("spread", "0.45"),
                ("return", "Mix"),
            ],
        ),
    });

    // ---- Rhythms: a sparse constellation is a pattern rather than a space. The MN3011's spacings
    // are not in multiple proportion, so two taps are not a dotted note - they are their own thing.
    all.push(p(
        "two-tap",
        "Two tap",
        &[
            ("time", "300 ms"),
            ("tap4", "80 %"),
            ("feedback", "55 %"),
            ("mix", "45 %"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "gallop",
        "Gallop",
        &[
            ("time", "340 ms"),
            ("tap2", "60 %"),
            ("tap3", "80 %"),
            ("feedback", "50 %"),
            ("mix", "40 %"),
            ("return", "Mix"),
        ],
    ));
    all.push(p(
        "scatter",
        "Scatter",
        &[
            ("line", "3328"),
            ("time", "420 ms"),
            ("tap1", "70 %"),
            ("tap3", "85 %"),
            ("tap5", "60 %"),
            ("spread", "1.30"),
            ("feedback", "52 %"),
            ("mix", "45 %"),
            ("return", "Mix"),
        ],
    ));
    all.push(Patch {
        slug: "three-against-two",
        name: "Three against two",
        set: vec![
            ("sync", "On".into()),
            ("change", "Snap".into()),
            ("time", at("1/8T")),
            ("tap2", "70 %".into()),
            ("tap4", "85 %".into()),
            ("feedback", "58 %".into()),
            ("mix", "45 %".into()),
            ("return", "Mix".into()),
        ],
    });
    all.push(Patch {
        slug: "backbeat",
        name: "Backbeat",
        set: vec![
            ("line", "8192".into()),
            ("sync", "On".into()),
            ("change", "Snap".into()),
            ("time", at("1/4")),
            ("tap5", "75 %".into()),
            ("feedback", "60 %".into()),
            ("mix", "45 %".into()),
            ("return", "Mix".into()),
        ],
    });
    all.push(p(
        "wide-scatter",
        "Wide scatter",
        &[
            ("line", "8192"),
            ("time", "700 ms"),
            ("tap1", "65 %"),
            ("tap4", "80 %"),
            ("spread", "1.80"),
            ("feedback", "62 %"),
            ("mix", "48 %"),
            ("routing", "Ping-pong"),
            ("return", "Mix"),
        ],
    ));
    all.push(p(
        "comb-rhythm",
        "Comb rhythm",
        &[
            ("line", "3328"),
            ("time", "150 ms"),
            ("tap3", "70 %"),
            ("tap4", "80 %"),
            ("tap5", "90 %"),
            ("spread", "0.70"),
            ("feedback", "55 %"),
            ("mix", "45 %"),
            ("return", "Mix"),
        ],
    ));

    // ---- Modulation. Moving the clock is how this chip family makes chorus and vibrato, so these
    // are the same device rather than a second effect bolted on.
    all.push(p(
        "chorus",
        "Chorus",
        &[
            ("line", "1024"),
            ("time", "18 ms"),
            ("feedback", "0 %"),
            ("mix", "50 %"),
            ("wobble", "55 %"),
            ("rate", "0.600 Hz"),
            ("filter", "Tracking"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "vibrato",
        "Vibrato",
        &[
            ("line", "1024"),
            ("time", "12 ms"),
            ("feedback", "0 %"),
            ("mix", "100 %"),
            ("wobble", "45 %"),
            ("rate", "4.50 Hz"),
            ("filter", "Tracking"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "deep-flange",
        "Deep flange",
        &[
            ("line", "1024"),
            ("time", "10 ms"),
            ("feedback", "70 %"),
            ("mix", "50 %"),
            ("wobble", "30 %"),
            ("rate", "0.150 Hz"),
            ("filter", "Tracking"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "seasick",
        "Seasick",
        &[
            ("time", "420 ms"),
            ("feedback", "60 %"),
            ("mix", "55 %"),
            ("wobble", "95 %"),
            ("rate", "0.900 Hz"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "warble-tape",
        "Warble tape",
        &[
            ("time", "300 ms"),
            ("feedback", "58 %"),
            ("mix", "45 %"),
            ("wobble", "25 %"),
            ("rate", "1.60 Hz"),
            ("bias", "+25 %"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "slow-drift",
        "Slow drift",
        &[
            ("line", "8192"),
            ("time", "800 ms"),
            ("feedback", "68 %"),
            ("mix", "42 %"),
            ("wobble", "18 %"),
            ("rate", "0.080 Hz"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "wow-and-flutter",
        "Wow and flutter",
        &[
            ("time", "450 ms"),
            ("feedback", "55 %"),
            ("mix", "45 %"),
            ("wobble", "40 %"),
            ("rate", "2.80 Hz"),
            ("bias", "+18 %"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "detune-double",
        "Detune double",
        &[
            ("line", "1024"),
            ("time", "22 ms"),
            ("feedback", "0 %"),
            ("mix", "45 %"),
            ("wobble", "12 %"),
            ("rate", "0.300 Hz"),
            ("filter", "Tracking"),
            ("return", "Tail"),
        ],
    ));

    // ---- Reverse. The window is filled forwards and read out backwards; the loop's own return
    // is not reversed, so the repeats swim back the other way.
    all.push(p(
        "reverse-swell",
        "Reverse swell",
        &[
            ("line", "8192"),
            ("time", "750 ms"),
            ("tap3", "30 %"),
            ("tap5", "55 %"),
            ("feedback", "50 %"),
            ("mix", "60 %"),
            ("reverse", "Reverse"),
            ("return", "Mix"),
        ],
    ));
    all.push(p(
        "reverse-stab",
        "Reverse stab",
        &[
            ("time", "300 ms"),
            ("feedback", "30 %"),
            ("mix", "65 %"),
            ("reverse", "Reverse"),
            ("return", "Tail"),
        ],
    ));
    all.push(Patch {
        slug: "backwards-wash",
        name: "Backwards wash",
        set: with(
            six_open(),
            &[
                ("line", "8192"),
                ("time", "600 ms"),
                ("feedback", "70 %"),
                ("mix", "60 %"),
                ("reverse", "Reverse"),
                ("return", "Mix"),
            ],
        ),
    });
    all.push(Patch {
        slug: "reverse-rhythm",
        name: "Reverse rhythm",
        set: vec![
            ("line", "3328".into()),
            ("sync", "On".into()),
            ("change", "Snap".into()),
            ("time", at("1/8")),
            ("tap4", "70 %".into()),
            ("feedback", "55 %".into()),
            ("mix", "55 %".into()),
            ("reverse", "Reverse".into()),
            ("return", "Mix".into()),
        ],
    });

    // ---- The warts, reachable on purpose. Below the datasheet's clock, off the bias trimmer, and
    // past the point where the line sings on its own noise - all three are the device, not damage.
    all.push(p(
        "below-spec",
        "Below spec",
        &[
            ("line", "8192"),
            ("time", "1.80 s"),
            ("feedback", "60 %"),
            ("mix", "50 %"),
            ("filter", "Fixed"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "gritty",
        "Gritty",
        &[
            ("time", "350 ms"),
            ("feedback", "65 %"),
            ("mix", "50 %"),
            ("bias", "+85 %"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "starved",
        "Starved",
        &[
            ("time", "400 ms"),
            ("feedback", "62 %"),
            ("mix", "48 %"),
            ("bias", "-80 %"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "lo-fi-short",
        "Lo-fi short",
        &[
            ("line", "1024"),
            ("time", "40 ms"),
            ("feedback", "70 %"),
            ("mix", "55 %"),
            ("bias", "+60 %"),
            ("filter", "Fixed"),
            ("return", "Tail"),
        ],
    ));
    all.push(p(
        "singing-line",
        "Singing line",
        &[
            ("time", "500 ms"),
            ("feedback", "92 %"),
            ("mix", "45 %"),
            ("return", "Tail"),
        ],
    ));
    all.push(Patch {
        slug: "noise-floor",
        name: "Noise floor",
        set: with(
            six_open(),
            &[
                ("line", "8192"),
                ("time", "1.20 s"),
                ("feedback", "85 %"),
                ("mix", "40 %"),
                ("return", "Mix"),
            ],
        ),
    });

    // ---- Routing. Ping-pong is a change to the feedback matrix and nothing else, which is why it
    // costs nothing in character.
    all.push(Patch {
        slug: "ping-pong",
        name: "Ping-pong",
        set: vec![
            ("line", "3328".into()),
            ("sync", "On".into()),
            ("time", at("1/8")),
            ("feedback", "70 %".into()),
            ("mix", "45 %".into()),
            ("routing", "Ping-pong".into()),
            ("filter", "Tracking".into()),
            ("return", "Tail".into()),
        ],
    });
    all.push(Patch {
        slug: "wide-ping-pong",
        name: "Wide ping-pong",
        set: vec![
            ("line", "8192".into()),
            ("sync", "On".into()),
            ("change", "Snap".into()),
            ("time", at("1/4")),
            ("feedback", "72 %".into()),
            ("mix", "50 %".into()),
            ("spread", "1.40".into()),
            ("routing", "Ping-pong".into()),
            ("return", "Tail".into()),
        ],
    });
    all.push(p(
        "mono-tight",
        "Mono tight",
        &[
            ("line", "1024"),
            ("time", "150 ms"),
            ("feedback", "50 %"),
            ("mix", "40 %"),
            ("routing", "Mono"),
            ("return", "Tail"),
        ],
    ));

    all
}

fn main() {
    let patches = catalogue();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("presets");
    std::fs::create_dir_all(&dir).expect("the presets directory");

    // Every slug and every name has to be its own: two files with one name is a browser with a
    // duplicate row, and two patches with one slug is one file silently overwriting another.
    for (i, patch) in patches.iter().enumerate() {
        for other in &patches[i + 1..] {
            assert_ne!(patch.slug, other.slug, "two patches share a file name");
            assert_ne!(patch.name, other.name, "two patches share a display name");
        }
    }

    let host = ApplyingHost;
    let setter = ParamSetter::new(&host);
    let mut listing = Vec::new();

    for patch in &patches {
        // A fresh Init each time: a preset states its difference from the plain echo, and carrying
        // the previous patch's values over would make every entry depend on the one above it.
        let params = MxmBucketDelayParams::default();
        let bounds = mxm_bucket_delay::editor::sections::all_parameters(&params);

        for (id, text) in &patch.set {
            let bound = bounds
                .iter()
                .find(|b| b.id == *id)
                .unwrap_or_else(|| panic!("{}: `{id}` is not a parameter", patch.name));
            let normalised = bound.param.parse(text).unwrap_or_else(|| {
                panic!(
                    "{}: `{id}` will not accept {text:?} - it formats as {:?}",
                    patch.name,
                    bound.param.text()
                )
            });
            bound.param.set(&setter, normalised);
        }

        let preset = Preset::capture(patch.name, Category::Fx, &params);
        let path = dir.join(format!("{}.json", patch.slug));
        std::fs::write(&path, preset.to_json()).expect("writing the preset");
        listing.push((patch.name, patch.slug));
    }

    println!("{} presets written to {}\n", patches.len(), dir.display());
    println!("The list for `preset.rs`, in this file's own order:\n");
    println!("pub const FACTORY_FILES: &[(&str, &str)] = &[");
    for (name, slug) in &listing {
        println!("    (\"{name}\", include_str!(\"../presets/{slug}.json\")),");
    }
    println!("];");
}
