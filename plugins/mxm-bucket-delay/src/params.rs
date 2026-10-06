//! Parameter definitions.
//!
//! Every `#[id]` here is **permanent**. Changing one breaks every saved project that used the
//! plugin, so ids are part of the public interface.
//!
//! # Twenty-one controls, and why that is not too many
//!
//! The owner's ruling, 2026-09-05: *there is no goal to have few parameters as long as they all do
//! something*. A six-tap delay is not a three-knob effect. What makes twenty-one legible is that they
//! fall into three groups — the line, the taps, and the dirt — and that **not one of them is a
//! second way to do something another already does**.
//!
//! The one that decides the rest is [`time`](MxmBucketDelayParams::time), because it is not a delay
//! control at all. It is the **clock**, and in a bucket brigade the clock sets delay, bandwidth,
//! noise and pitch-bend-on-turn together. Nothing here can set any of those four independently, and
//! a design that offered to would have built a different product.
//!
//! # Smoothing, and the two things that cannot be smoothed
//!
//! Mix and the tap faders multiply into audio and are smoothed per sample. Time is smoothed
//! *because* it bends pitch: the smoother is the glide, and a jump would be a click rather than a
//! tape-like slide.
//!
//! **`Line` and the three mode switches cannot be.** A line is a different chip with a different
//! length; `Routing` can bring a line into use; `Reverse` makes the buffered window meaningless;
//! `Return` swaps which branch feeds the loop, along with its reconstruction filter and its
//! expander state. The DSP fades the wet around each of them (`mxm_bucket_delay_dsp::unit`), which
//! is `mxm-folded-spring`'s fade-out, swap, fade-in and not a crossfade.

use mxm_bucket_delay_dsp::bbd::FilterMode;
use mxm_bucket_delay_dsp::{Line, Return, Routing};
use mxm_preset::PresetIdentity;
use nice_plug::prelude::*;
use std::sync::{Arc, RwLock};

/// Where the mix sits when the delay is inserted.
///
/// **Chosen**, and the collection's *an effect starts engaged* rule: an effect that starts silent
/// reads as broken. **Half**, the owner's ruling of 2026-09-06: on a *crossfade* the middle is the
/// honest place to open, because it is the one setting that commits to neither end and shows the
/// control has two of them. An earlier 0.35 was reasoned from the added level it replaced — *enough
/// to hear without burying the source* — which is the right instinct for a wet you add on top and
/// the wrong one for a balance.
pub const DEFAULT_MIX: f32 = 0.5;

/// The delay the plugin opens at, in seconds.
///
/// **Chosen**: a quarter of a second is the middle of what a bucket brigade does, and on the
/// default line (4096 stages) it puts the clock at 8.2 kHz — inside the MN3005's own working range
/// rather than in the below-spec region the brief keeps but labels.
pub const DEFAULT_TIME_S: f32 = 0.25;

/// The shortest and longest the Time control reaches.
///
/// **Wider than the hardware at both ends, deliberately** — the owner's ruling that the below-spec
/// clock region ships, labelled. At 8192 stages the slow end is a clock the catalogue would not
/// recognise, and that is where the device stops being an echo and starts being a texture. The DSP
/// clamps the clock itself to what the MN3101 could actually generate, so the ends of this range
/// are honest about being ends.
pub const MIN_TIME_S: f32 = 0.01;
/// See [`MIN_TIME_S`].
pub const MAX_TIME_S: f32 = 2.0;

/// The Time control's own range, as one value both the parameter and the synced mode read.
///
/// **Named because synced mode reads the knob's *position* on it**, not the seconds it would ask
/// for: a subdivision is chosen by where the knob sits, so the division a patch stores survives a
/// tempo change. Picking the division nearest in *seconds* would have let a patch saved as 1/8 at
/// 120 come back as 1/16. at 90, which is the one thing a synced delay must not do.
pub fn time_range() -> FloatRange {
    FloatRange::Skewed {
        min: MIN_TIME_S,
        max: MAX_TIME_S,
        factor: FloatRange::skew_factor(-1.5),
    }
}

/// **Time's tempo sync** (`plans/plan-tempo-sync-controls.md`): 1/32 to a half note on the collection's
/// one ladder, the top the longest. This plugin's own eleven-step table was exactly this span, so every
/// stored Time position keeps its subdivision. Which of them this tempo and this chip can hold is
/// [`time_bounds`]'s, clamped, never rescaled.
pub const TIME_SYNC: mxm_tempo::Ladder = mxm_tempo::Ladder::new(
    mxm_tempo::Span::new(mxm_tempo::Division::ThirtySecond, mxm_tempo::Division::Half),
    mxm_tempo::Direction::Time,
);

/// **The wobble Rate's tempo sync**: every LFO's ladder, the top the fastest.
pub const RATE_SYNC: mxm_tempo::Ladder =
    mxm_tempo::Ladder::new(mxm_tempo::Span::LFO, mxm_tempo::Direction::Rate);

/// **The delay a Time can be on this chip**, in seconds: the chip's own reach against its clock
/// (`Line::delay_bounds`) inside the control's range. What a synced Time's subdivisions are clamped
/// to — the owner's *"in accordance to the sequencer tempo and the number of buckets"*.
pub fn time_bounds(line: Line) -> (f64, f64) {
    let (short, long) = line.delay_bounds();
    (
        f64::from(short.max(MIN_TIME_S)),
        f64::from(long.min(MAX_TIME_S)),
    )
}

/// **What a new tempo or subdivision does to a synced tail** (the owner's Revision 2 split of the
/// old three-way Sync, `plans/plan-tempo-sync-controls.md`): Sync is the one on/off every synced
/// control has, and this is the transition law beside it. Inert while Sync is off, so a free delay is
/// the build before the split to the bit.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeChoice {
    /// A change **glides** the tail into the new time, because time *is* clock rate. The tape
    /// behaviour, and the default.
    #[id = "glide"]
    #[name = "Glide"]
    Glide,
    /// The change is taken immediately. In time, and audibly digital.
    #[id = "snap"]
    #[name = "Snap"]
    Snap,
}

/// Which bucket brigade is fitted — a mirror of the DSP's own [`Line`], because `#[id]` and
/// `#[name]` are this plugin's public interface and the DSP crate must not have to care what a host
/// writes into a project file.
///
/// The names are stage counts rather than part numbers: a parameter label may not carry another
/// maker's model designation, and *1024* says the thing that matters anyway.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineChoice {
    #[id = "1024"]
    #[name = "1024"]
    S1024,
    #[id = "3328"]
    #[name = "3328"]
    S3328,
    #[id = "4096"]
    #[name = "4096"]
    S4096,
    #[id = "8192"]
    #[name = "8192"]
    S8192,
}

impl LineChoice {
    pub const ALL: [Self; 4] = [Self::S1024, Self::S3328, Self::S4096, Self::S8192];

    pub const fn line(self) -> Line {
        match self {
            Self::S1024 => Line::S1024,
            Self::S3328 => Line::S3328,
            Self::S4096 => Line::S4096,
            Self::S8192 => Line::S8192,
        }
    }
}

/// What feeds the loop.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnChoice {
    /// The bank's sum — dense, and builds into a wash.
    #[id = "mix"]
    #[name = "Mix"]
    Mix,
    /// The last tap alone — a clean long repeat under a busy output.
    #[id = "tail"]
    #[name = "Tail"]
    Tail,
}

impl ReturnChoice {
    pub const fn mode(self) -> Return {
        match self {
            Self::Mix => Return::Mix,
            Self::Tail => Return::Tail,
        }
    }
}

/// Mono, stereo or ping-pong.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingChoice {
    #[id = "mono"]
    #[name = "Mono"]
    Mono,
    #[id = "stereo"]
    #[name = "Stereo"]
    Stereo,
    #[id = "pingpong"]
    #[name = "Ping-pong"]
    PingPong,
}

impl RoutingChoice {
    pub const fn routing(self) -> Routing {
        match self {
            Self::Mono => Routing::Mono,
            Self::Stereo => Routing::Stereo,
            Self::PingPong => Routing::PingPong,
        }
    }
}

/// Which filters are fitted around the line.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterChoice {
    /// Sallen-Key at the hardware's 2 kHz, chosen for the longest delay — so short delays stay
    /// dark, because the filter does not know how fast the clock is running.
    #[id = "fixed"]
    #[name = "Fixed"]
    Fixed,
    /// Switched-capacitor: the cutoff follows the clock, so short delays stay bright. A minority of
    /// real designs do this, and both are real designs.
    #[id = "tracking"]
    #[name = "Tracking"]
    Tracking,
}

impl FilterChoice {
    pub const fn mode(self) -> FilterMode {
        match self {
            Self::Fixed => FilterMode::Fixed,
            Self::Tracking => FilterMode::Tracking,
        }
    }
}

/// Formats a parameter value for display.
type ValueToString = Arc<dyn Fn(f32) -> String + Send + Sync>;
/// Parses a typed-in value, returning `None` if it cannot be understood.
type StringToValue = Arc<dyn Fn(&str) -> Option<f32> + Send + Sync>;

fn v2s_percent() -> ValueToString {
    Arc::new(|v| format!("{:.0} %", v * 100.0))
}

fn s2v_percent() -> StringToValue {
    Arc::new(|text| {
        text.trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v / 100.0)
    })
}

/// Milliseconds below a second, seconds to two decimals from one.
///
/// **The branch is chosen from the rounding the finer branch prints**, never from the raw value. A
/// host formats a value, parses the text and formats what the parameter's normalized inverse gives
/// back: at a raw `v < 1.0` test, 0.9996 s printed `1000 ms`, parsed to exactly one second, and came
/// back as `1.00 s` — or the mirror, whichever side of one the inverse lands. Deciding on the
/// rounded millisecond means every reading of one second is `1.00 s`, and parsing it cannot cross.
fn v2s_ms() -> ValueToString {
    Arc::new(|v| {
        let ms = v * 1000.0;
        if ms.round() >= 1000.0 {
            format!("{v:.2} s")
        } else {
            format!("{ms:.0} ms")
        }
    })
}

fn s2v_ms() -> StringToValue {
    Arc::new(|text| {
        let t = text.trim().to_ascii_lowercase();
        if let Some(rest) = t.strip_suffix("ms") {
            rest.trim().parse::<f32>().ok().map(|v| v / 1000.0)
        } else {
            t.trim_end_matches('s').trim().parse::<f32>().ok()
        }
    })
}

/// The bias trimmer's own reading: centre is the distortion minimum every unit left the factory
/// trimmed to, and both directions grit up.
fn v2s_bias() -> ValueToString {
    Arc::new(|v| {
        if v.abs() < 0.005 {
            "Trimmed".to_string()
        } else {
            format!("{:+.0} %", v * 100.0)
        }
    })
}

fn s2v_bias() -> StringToValue {
    Arc::new(|text| {
        let t = text.trim();
        if t.eq_ignore_ascii_case("trimmed") {
            return Some(0.0);
        }
        t.trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v / 100.0)
    })
}

fn v2s_hz() -> ValueToString {
    // Three decimals under 1 Hz. The wobble's slow end has settings a hundredth of a hertz apart,
    // and two decimals there round several distinct values onto the same text.
    //
    // **The branch is the three-decimal reading's, not the raw value's**, for the reason `v2s_ms`
    // gives: at `v < 1.0`, 0.9996 Hz printed `1.000 Hz`, parsed to one, and came back `1.00 Hz`.
    Arc::new(|v| {
        if (v * 1000.0).round() >= 1000.0 {
            format!("{v:.2} Hz")
        } else {
            format!("{v:.3} Hz")
        }
    })
}

fn s2v_hz() -> StringToValue {
    // **Case-insensitively**, because the unit is written `Hz` and the first version of this
    // stripped only `hz` - so every value this plugin formatted was one its own parser rejected.
    // `clap-validator`'s `param-conversions` found it; a host would have met it as a text box that
    // refuses what it has just displayed.
    Arc::new(|text| {
        let t = text.trim().to_ascii_lowercase();
        t.trim_end_matches("hz").trim().parse::<f32>().ok()
    })
}

/// One tap fader, by index.
fn tap(index: usize, default: f32) -> FloatParam {
    FloatParam::new(
        format!("Tap {}", index + 1),
        default,
        FloatRange::Linear { min: 0.0, max: 1.0 },
    )
    .with_smoother(SmoothingStyle::Linear(10.0))
    .with_value_to_string(v2s_percent())
    .with_string_to_value(s2v_percent())
}

/// The reference reverberation circuit's own mixing ladder, as gains.
///
/// Panasonic mix the MN3011's six taps through 100, 110, 120, 130, 140 and 150 kΩ (catalogue
/// p. 45), so tap gain falls as `1/R`: **computed**, 0, −0.83, −1.58, −2.28, −2.92 and −3.52 dB. A
/// gentle 3.5 dB tilt across the whole set and not a decay envelope — the reverberation comes from
/// the *spacing*, not from the weighting.
///
/// It lives here as the shape a preset can open rather than in the DSP: with the faders exposed,
/// the faders **are** the ladder.
pub const LADDER: [f32; 6] = [1.0, 0.909, 0.833, 0.769, 0.714, 0.667];

#[derive(Params)]
pub struct MxmBucketDelayParams {
    /// The clock. Sets delay, and with it bandwidth, noise and pitch-bend-on-turn.
    #[id = "time"]
    pub time: FloatParam,
    /// Loop gain. Sings in the last tenth, as the hardware does on its own noise.
    #[id = "feedback"]
    pub feedback: FloatParam,
    /// **The mix**: the dry alone at nought, the echo alone at one. **Nought is Off.**
    #[id = "mix"]
    pub mix: FloatParam,

    #[id = "tap1"]
    pub tap1: FloatParam,
    #[id = "tap2"]
    pub tap2: FloatParam,
    #[id = "tap3"]
    pub tap3: FloatParam,
    #[id = "tap4"]
    pub tap4: FloatParam,
    #[id = "tap5"]
    pub tap5: FloatParam,
    #[id = "tap6"]
    pub tap6: FloatParam,

    /// Which chip is fitted: length, noise floor, bandwidth and distortion, together.
    #[id = "line"]
    pub line: EnumParam<LineChoice>,
    /// Stretches the tap constellation about tap 6. 1.0 is Panasonic's ratios.
    #[id = "spread"]
    pub spread: FloatParam,
    /// What feeds the loop: the bank's sum, or the last tap alone.
    #[id = "return"]
    pub return_mode: EnumParam<ReturnChoice>,

    /// The trimmer. Centre is the measured distortion minimum; both directions grit up.
    #[id = "bias"]
    pub bias: FloatParam,
    /// Clock modulation depth.
    #[id = "wobble"]
    pub wobble: FloatParam,
    /// Wobble rate: tape flutter at one end, seasick at the other.
    #[id = "rate"]
    pub rate: FloatParam,
    /// Tempo sync for the wobble's Rate: its position picks a division of the host's tempo.
    #[id = "ratesync"]
    pub rate_sync: BoolParam,
    /// Fixed at the hardware's 2 kHz, or tracking the clock.
    #[id = "filter"]
    pub filter: EnumParam<FilterChoice>,

    /// Mono, stereo or ping-pong.
    #[id = "routing"]
    pub routing: EnumParam<RoutingChoice>,
    /// Buffers and flips the external input into the line.
    #[id = "reverse"]
    pub reverse: BoolParam,
    /// Time's tempo sync. The id is the old three-way's; `filter_state` reads an old project's
    /// Free, Glide or Snap into this and [`Self::change`].
    #[id = "sync"]
    pub sync: BoolParam,
    /// What a synced tail does at a change: Glide or Snap.
    #[id = "change"]
    pub change: EnumParam<ChangeChoice>,

    /// Which preset is loaded, and what it looked like when it was.
    #[persist = "preset"]
    pub preset: RwLock<PresetIdentity>,
}

impl MxmBucketDelayParams {
    /// **The delay these controls and this transport are asking for**, in seconds.
    ///
    /// Here rather than in the plugin because the *editor* needs the same answer: the constellation
    /// is drawn at the delay in force, and the panel names the subdivision `Time` is selecting.
    /// `plugins/AGENTS.md` records the same rule for the loop gain — two copies of a law like this
    /// are free to disagree, and the disagreement is silent.
    pub fn target_time(&self, tempo: Option<f64>) -> f32 {
        // With no tempo, a synced delay is the knob again: one that fell silent in a host with no
        // transport would be broken rather than honest.
        let (lo, hi) = time_bounds(self.line.value().line());
        TIME_SYNC
            .resolve(self.sync.value(), tempo, self.time_position(), lo, hi)
            .map_or_else(|| self.time.value(), |seconds| seconds as f32)
    }

    /// The subdivision `Time` is selecting at this tempo, on the chip that is fitted.
    pub fn division_at(&self, bpm: f64) -> mxm_tempo::Division {
        let (lo, hi) = time_bounds(self.line.value().line());
        TIME_SYNC.division(self.time_position(), bpm, lo, hi)
    }

    /// Time's **modulated** position across its travel, which is what picks a subdivision: a host's
    /// modulation picks the subdivision a moved knob would (the editor reads the unmodulated one).
    fn time_position(&self) -> f32 {
        use nice_plug::prelude::Param as _;
        self.time.modulated_normalized_value()
    }
}

impl Default for MxmBucketDelayParams {
    /// The init patch: **one chip, one tap, engaged, and trimmed.** A plain quarter-second echo on
    /// the 4096-stage part, which is what a bucket brigade delay is before anything is opened up.
    /// The six-tap constellation is a preset away, not a default — it is the most interesting thing
    /// the family can do and it should be found rather than arrived in.
    fn default() -> Self {
        Self {
            time: FloatParam::new("Time", DEFAULT_TIME_S, time_range())
                // Time is the clock, so a change *bends pitch*. The smoother is the glide, and it is
                // long for that reason rather than for zipper noise.
                .with_smoother(SmoothingStyle::Linear(120.0))
                .with_value_to_string(v2s_ms())
                .with_string_to_value(s2v_ms()),

            feedback: FloatParam::new("Feedback", 0.35, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(v2s_percent())
                .with_string_to_value(s2v_percent()),

            mix: FloatParam::new(
                "Mix",
                DEFAULT_MIX,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_value_to_string(v2s_percent())
            .with_string_to_value(s2v_percent()),

            tap1: tap(0, 0.0),
            tap2: tap(1, 0.0),
            tap3: tap(2, 0.0),
            tap4: tap(3, 0.0),
            tap5: tap(4, 0.0),
            tap6: tap(5, 1.0),

            line: EnumParam::new("Line", LineChoice::S4096),
            spread: FloatParam::new("Spread", 1.0, FloatRange::Linear { min: 0.0, max: 2.0 })
                .with_smoother(SmoothingStyle::Linear(30.0))
                .with_value_to_string(Arc::new(|v| format!("{v:.2}×")))
                .with_string_to_value(Arc::new(|t| {
                    t.trim().trim_end_matches('×').trim().parse::<f32>().ok()
                })),
            return_mode: EnumParam::new("Return", ReturnChoice::Mix),

            bias: FloatParam::new(
                "Bias",
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(30.0))
            .with_value_to_string(v2s_bias())
            .with_string_to_value(s2v_bias()),
            wobble: FloatParam::new("Wobble", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0))
                .with_value_to_string(v2s_percent())
                .with_string_to_value(s2v_percent()),
            rate: FloatParam::new(
                "Rate",
                0.4,
                FloatRange::Skewed {
                    min: 0.05,
                    max: 12.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_smoother(SmoothingStyle::Linear(30.0))
            .with_value_to_string(v2s_hz())
            .with_string_to_value(s2v_hz()),
            filter: EnumParam::new("Filter", FilterChoice::Fixed),

            routing: EnumParam::new("Routing", RoutingChoice::Stereo),
            // **Named rather than left as On/Off.** The editor's own switch reads *Forward* and
            // *Reverse*, and a host's generic UI showing *Off* for the same control would be a
            // second vocabulary for one parameter. It also makes a captured preset's `text`
            // round-trip, which is what that field claims to be.
            reverse: BoolParam::new("Reverse", false)
                .with_value_to_string(Arc::new(|v| {
                    if v { "Reverse" } else { "Forward" }.to_string()
                }))
                .with_string_to_value(Arc::new(|text| {
                    let t = text.trim();
                    if t.eq_ignore_ascii_case("reverse") || t.eq_ignore_ascii_case("true") {
                        Some(true)
                    } else if t.eq_ignore_ascii_case("forward") || t.eq_ignore_ascii_case("false") {
                        Some(false)
                    } else {
                        None
                    }
                })),
            sync: BoolParam::new("Time sync", false),
            change: EnumParam::new("Change", ChangeChoice::Glide),
            rate_sync: BoolParam::new("Rate sync", false),

            preset: RwLock::new(PresetIdentity::none()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each mirror covers its DSP type exactly. A mirror that lost a variant would silently map two
    /// ids onto one sound; one that gained a stale variant would name a chip that is not fitted.
    #[test]
    fn every_mirror_covers_its_dsp_type_exactly() {
        let lines: Vec<Line> = LineChoice::ALL.iter().map(|l| l.line()).collect();
        for line in Line::ALL {
            assert!(lines.contains(&line), "{line:?} cannot be selected");
        }
        assert_eq!(lines.len(), Line::ALL.len());

        assert_eq!(ReturnChoice::Mix.mode(), Return::Mix);
        assert_eq!(ReturnChoice::Tail.mode(), Return::Tail);
        assert_eq!(RoutingChoice::Mono.routing(), Routing::Mono);
        assert_eq!(RoutingChoice::Stereo.routing(), Routing::Stereo);
        assert_eq!(RoutingChoice::PingPong.routing(), Routing::PingPong);
    }

    /// **Twenty-one controls.** Twenty until `Division` was folded into `Time` (2026-09-06), then
    /// nineteen; the owner's one tempo-sync interface (2026-09-25) split the three-way Sync into
    /// Sync and Change and gave the wobble's Rate its own Sync.
    #[test]
    fn there_are_exactly_twenty_one_controls() {
        let params = MxmBucketDelayParams::default();
        let count = params.param_map().len();
        assert_eq!(count, 21, "the inventory moved: {count} parameters");
    }

    /// The subdivision a Time *position* picks on `line` at `bpm`.
    fn at_time(seconds: f32, bpm: f64, line: Line) -> mxm_tempo::Division {
        let (lo, hi) = time_bounds(line);
        TIME_SYNC.division(time_range().normalize(seconds), bpm, lo, hi)
    }

    /// **`Time` selects the subdivision when synced, and the reachable set is the tempo's and the
    /// chip's.** The owner's report was that a snapped delay was *"hardcoded to 1/8"*.
    #[test]
    fn time_sweeps_the_subdivisions_a_tempo_and_a_chip_can_reach() {
        // The knob's travel covers all eleven where the chip and the range allow it.
        let seen: Vec<mxm_tempo::Division> = (0..=100)
            .map(|n| {
                at_time(
                    time_range().unnormalize(n as f32 / 100.0),
                    120.0,
                    Line::S4096,
                )
            })
            .collect();
        for &d in TIME_SYNC.span.divisions() {
            assert!(
                seen.contains(&d),
                "{d:?} is not reachable at 120 bpm on 4096 stages"
            );
        }
        // Ordered: turning the knob up never shortens the delay.
        for pair in seen.windows(2) {
            assert!(
                pair[1].beats() >= pair[0].beats(),
                "the sweep went backwards"
            );
        }

        // The short chip cannot hold a half note at 120 bpm — 1.0 s against its own 0.73 s ceiling
        // — so the knob's top gives the longest it *can* hold rather than a lie.
        let top = at_time(MAX_TIME_S, 120.0, Line::S1024);
        assert!(top.seconds(120.0) as f32 <= Line::S1024.delay_bounds().1);
        assert!(
            at_time(MAX_TIME_S, 120.0, Line::S8192).beats() > top.beats(),
            "a longer chip has to reach further"
        );

        // And the tempo moves it: at 40 bpm a half note is three seconds, past every chip and past
        // the control's own range.
        let slow = at_time(MAX_TIME_S, 40.0, Line::S8192);
        assert!(
            slow.seconds(40.0) as f32 <= MAX_TIME_S,
            "{slow:?} is past the range"
        );
    }

    /// **Host modulation picks the subdivision**, as a moved knob would: the synced delay follows the
    /// modulated position, not the one the knob was set to.
    #[test]
    fn host_modulation_of_time_picks_the_subdivision() {
        use nice_plug::params::InternalParamMut;
        let params = MxmBucketDelayParams::default();
        unsafe {
            let _ = params.sync._internal_set_normalized_value(1.0);
            let _ = params.time._internal_set_normalized_value(0.2);
        }
        let set_at = params.target_time(Some(120.0));
        unsafe {
            let _ = params.time._internal_modulate_value(0.6);
        }
        let (lo, hi) = time_bounds(params.line.value().line());
        let modulated = TIME_SYNC.division(0.8, 120.0, lo, hi).seconds(120.0) as f32;
        assert_eq!(params.target_time(Some(120.0)), modulated);
        assert!(modulated > set_at, "modulation up picked no longer delay");
    }

    /// A subdivision picked by knob *position* stays the same subdivision when the tempo changes,
    /// which is the whole point of syncing. Picking the nearest in seconds would not.
    #[test]
    fn the_subdivision_a_patch_stores_survives_a_tempo_change() {
        let stored = time_range().unnormalize(0.5);
        let at_120 = at_time(stored, 120.0, Line::S4096);
        for &bpm in &[70.0, 90.0, 140.0, 175.0] {
            assert_eq!(
                at_time(stored, bpm, Line::S4096),
                at_120,
                "the same knob position gave a different subdivision at {bpm} bpm"
            );
        }
    }

    /// The init patch is a plain quarter-second echo on one chip: engaged, trimmed, one tap open.
    #[test]
    fn the_default_is_a_plain_echo_engaged_and_trimmed() {
        let p = MxmBucketDelayParams::default();
        assert!(
            p.mix.value() > 0.0,
            "an inserted effect demonstrates itself"
        );
        assert_eq!(p.bias.value(), 0.0, "every unit left the factory trimmed");
        assert_eq!(p.tap6.value(), 1.0);
        for tap in [&p.tap1, &p.tap2, &p.tap3, &p.tap4, &p.tap5] {
            assert_eq!(
                tap.value(),
                0.0,
                "the constellation is a preset, not a default"
            );
        }
        assert!(!p.sync.value(), "the hardware does not know the tempo");
        assert!(!p.rate_sync.value());
        assert_eq!(p.change.value(), ChangeChoice::Glide);
        assert!(!p.reverse.value());
        // At 120 bpm an eighth is 250 ms, which is where the delay opens.
        assert!((mxm_tempo::Division::Eighth.seconds(120.0) - 0.25).abs() < 1e-9);
    }

    /// Plain values either side of a formatter's branch point: the point, and fractions of the
    /// **finer** branch's printed step around it, both halves of its rounding included.
    fn around(point: f32, step: f32) -> impl Iterator<Item = f32> {
        [
            -1.0, -0.6, -0.5, -0.49, -0.4, -0.1, 0.0, 0.1, 0.4, 0.49, 0.5, 0.6, 1.0,
        ]
        .into_iter()
        .map(move |k| point + k * step)
    }

    /// Every place a formatter here changes its unit, its precision or its words, in plain units,
    /// with the finer branch's printed step.
    fn branch_points(id: &str) -> Vec<(f32, f32)> {
        match id {
            // `v2s_ms`: milliseconds to the unit below a second, seconds to two decimals above.
            "time" => vec![(1.0, 0.001)],
            // `v2s_hz`: three decimals below a hertz, two above.
            "rate" => vec![(1.0, 0.001)],
            // `v2s_bias`: "Trimmed" where the percentage rounds to zero, a signed whole percent out.
            "bias" => vec![(-0.005, 0.0001), (0.005, 0.0001)],
            _ => Vec::new(),
        }
    }

    /// One trip through the host, as `vendor/nice-plug`'s CLAP wrapper makes it: the CLAP value is
    /// the normalized value times the step count, the text carries the unit, and the parsed text
    /// comes back through the parameter's normalized conversion before it is formatted again.
    /// Returns the failure, if the text changed or did not parse.
    ///
    /// # Safety
    ///
    /// `ptr` must point at a parameter that outlives the call.
    unsafe fn host_round_trip(id: &str, ptr: ParamPtr, clap_value: f64) -> Option<String> {
        unsafe {
            let steps = ptr.step_count().unwrap_or(1);
            let normalised = clap_value as f32 / steps as f32;
            let first = ptr.normalized_value_to_string(normalised, true);
            let Some(parsed) = ptr.string_to_normalized_value(&first) else {
                return Some(format!("{id}: {first:?} does not parse"));
            };
            let back = parsed as f64 * steps as f64;
            let second = ptr.normalized_value_to_string(back as f32 / steps as f32, true);
            (second != first).then(|| {
                format!(
                    "{id} at plain {}: {first:?} came back {second:?}",
                    ptr.preview_plain(normalised)
                )
            })
        }
    }

    /// **Every value this plugin writes down, it reads back to the same text through the host's
    /// normalized conversion.** A host formats a value, parses the text, normalizes the result and
    /// formats that again, so the text must survive the parameter's normalized inverse — not merely
    /// come back within a tolerance. `clap-validator`'s `param-conversions` checks exactly this, on a
    /// grid of its own, so a clean validator run proves nothing about a sliver between its points.
    ///
    /// The first version of this test compared plain values within a tolerance with the unit off,
    /// which is not the host's path: it passed `Time` turning `1000 ms` into `1.00 s` and `Rate`
    /// turning `1.000 Hz` into `1.00 Hz` across their unit boundaries. Before that,
    /// `param-conversions` had found `Rate`'s parser rejecting its own `0.40 Hz`.
    ///
    /// Probes, for every parameter: the twenty-step grid; `clap-validator` 0.4.1's own grid, whose
    /// size follows the parameter count; both sides of every branch point in [`branch_points`]; and
    /// a hair either side of zero on a range that crosses it.
    #[test]
    fn every_parameter_text_is_idempotent_through_the_hosts_conversion() {
        let params = MxmBucketDelayParams::default();
        let map = params.param_map();
        let validator_points = 4000usize.div_ceil(map.len()).clamp(5, 100);
        let mut failures = Vec::new();
        for (id, ptr, _) in map {
            // SAFETY: `params` owns every parameter these pointers refer to and outlives the loop;
            // this is the access `param-conversions` makes through CLAP.
            unsafe {
                let steps = ptr.step_count().unwrap_or(1) as f64;
                let mut probes: Vec<f64> = (0..=19).map(|i| steps * i as f64 / 19.0).collect();
                probes.extend(
                    (0..validator_points)
                        .map(|i| steps * (i as f64 / (validator_points - 1) as f64)),
                );
                let mut plains: Vec<f32> = branch_points(&id)
                    .into_iter()
                    .flat_map(|(point, step)| around(point, step))
                    .collect();
                let (low, high) = (ptr.preview_plain(0.0), ptr.preview_plain(1.0));
                if low.min(high) < 0.0 && low.max(high) > 0.0 {
                    plains.extend([-1.0e-3, -1.0e-4, 0.0, 1.0e-4, 1.0e-3]);
                }
                probes.extend(
                    plains
                        .into_iter()
                        .map(|plain| ptr.preview_normalized(plain) as f64 * steps),
                );
                failures.extend(
                    probes
                        .into_iter()
                        .filter_map(|value| host_round_trip(&id, ptr, value)),
                );
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// The ladder is the circuit's, tilting gently rather than decaying, and it is a shape a preset
    /// opens rather than something the DSP applies behind the faders.
    #[test]
    fn the_ladder_is_a_gentle_tilt_and_not_a_decay() {
        assert_eq!(LADDER[0], 1.0);
        let span_db = 20.0 * (LADDER[5] / LADDER[0]).log10();
        assert!((span_db + 3.52).abs() < 0.05, "{span_db} dB across the set");
        for pair in LADDER.windows(2) {
            assert!(pair[1] < pair[0]);
        }
    }

    use mxm_plugin_test::time_text_checks;

    /// **A time reading survives the host's round trip across the switch from milliseconds to
    /// seconds**, the unit chosen from the rounded reading (`mxm_plugin_test::time_text_checks`).
    #[test]
    fn time_readings_survive_the_hosts_round_trip_across_the_second() {
        time_text_checks::time_readings_round_trip(&MxmBucketDelayParams::default(), &["time"]);
    }
}
