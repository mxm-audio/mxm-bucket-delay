//! The device itself: a fixed-length line clocked at its own rate, its six taps, its
//! sample-and-hold output, its noise and its distortion.
//!
//! Everything here is downstream of one fact — **the clock is the only time control there is** — so
//! nothing takes a delay time. It takes a [`Clock`], and the delay, the bandwidth, the noise colour
//! and the aliasing all follow from it.
//!
//! # The shape of one clock tick
//!
//! Per Holters & Parker, the line runs at the clock rate and the host's rate never enters it. The
//! host's sample is *held* while [`Bbd::process`] walks forward through the tick or ticks that fall
//! inside it, and at each tick:
//!
//! 1. the anti-alias filter, advanced to that instant, is read — this is the sampling;
//! 2. the device's own noise is added, at the input, where the chip's noise arises;
//! 3. the polynomial nonlinearity is applied once for the whole line, scaled to the part's own
//!    distortion figure and to where its bias trimmer is set;
//! 4. the sample is written to the line and the six taps are read from it;
//! 5. the tap mix becomes the held output, which the reconstruction filter sees as a constant until
//!    the next tick.
//!
//! The hold in step 5 is not a detail. Holding each sample for one clock period convolves the
//! output with a rectangle of width `T`, whose transform is a sinc: −3.92 dB at the line's own
//! Nyquist. Raffel & Smith measured the insertion gain of real parts falling to −4 to −6 dB there
//! and could not fully explain it; the hold accounts for most of it, for free, without fitting
//! anything to a datasheet curve.

use crate::modal::ModalLowpass;
use crate::{Clock, FIXED_CUTOFF_HZ, Rng, TRACKING_CUTOFF_RATIO, finite_or_zero, flush};

/// The longest line the product reaches: two MN3005s, which is what one MN3101 is specified to
/// drive (catalogue pp. 56–61). Every line is allocated at this length, so a `Line` change is an
/// index change and never an allocation.
pub const MAX_STAGES: usize = 8192;

/// How many stages a sample moves per clock *period*, and therefore the 2 in `N / (2 · f_CP)`.
///
/// The device is clocked with **two antiphase phases**, and charge moves one stage on each of them,
/// so a full period carries a sample through two stages and an `N`-stage line takes `N / 2` periods
/// rather than `N`. Writing it as a named constant rather than a bare `/ 2` is deliberate: the
/// first implementation here ticked one stage per period, which put every delay at exactly twice
/// the law and passed every test that did not measure against the catalogue.
pub const STAGES_PER_CLOCK_PERIOD: usize = 2;

/// The line's length in clock periods — one written sample per period, and the longest line is
/// [`MAX_STAGES`] stages, which is that many periods.
pub const MAX_TICKS: usize = MAX_STAGES / STAGES_PER_CLOCK_PERIOD;

/// The storage the line actually needs: **one slot more than the longest delay**.
///
/// The write happens before the taps are read, so a tap `MAX_TICKS` periods back would wrap all
/// the way round onto the slot just written and return the input with no delay at all. The extra
/// slot is what keeps the oldest sample alive until it has been read.
pub const BUFFER_SLOTS: usize = MAX_TICKS + 1;

/// The MN3011's six tap positions, in stages, read off catalogue p. 44 and confirmed against
/// p. 46's per-terminal delay table.
///
/// **Not one is a multiple of another, and that is the design.** Panasonic's own words: six taps
/// *"not in multiple proportion with each other so that a proper mixing of the six differently
/// delayed output signals generates a highly effective reverberation"*. Evenly spaced taps give
/// repeats; these give a spread. Every line in this product wears this constellation, scaled — it
/// is the most interesting thing the device family can do and it was never in a famous pedal.
pub const MN3011_TAP_STAGES: [usize; 6] = [396, 662, 1194, 1726, 2790, 3328];

/// The part the tap positions are proportions of.
pub const MN3011_STAGES: usize = 3328;

/// How many taps a line has.
pub const TAPS: usize = 6;

/// Raffel & Smith's polynomial nonlinearity and its two coefficients (DAFx-10 §4.5).
///
/// ```text
///            ⎧ 1 − a − b                      for x > 1
///   f(x)  =  ⎨ x − a·x² − b·x³ + a            for −1 < x < 1
///            ⎩ −1 − a + b                     for x < −1
/// ```
///
/// The `a·x²` term is what makes it **asymmetric**, so the second harmonic dominates, which is what
/// the paper's measured spectra show; the `+a` keeps the output averaging around zero. The paper is
/// candid that this underestimates the added harmonics as amplitude falls — the nonlinearity is
/// largely level-*independent*, which no ordinary waveshaper is.
pub mod nonlinearity {
    /// The quadratic coefficient — the asymmetry, and therefore the second harmonic.
    pub const A: f32 = 1.0 / 8.0;
    /// The cubic coefficient.
    pub const B: f32 = 1.0 / 18.0;

    /// The upper bound, exactly. Every saturator in this collection is bounded *in `f32`*, clamped
    /// rather than merely approaching, because the loop's boundedness argument rests on it.
    ///
    /// **One declared correction to the paper.** Raffel & Smith give the clipping cases as
    /// `1 − a − b` and `−1 − a + b`, and say in the text that they are *"formulated so that the
    /// nonlinearity smoothly transitions between potential input signal ranges"* — but they do not
    /// meet the polynomial, which reaches `1 − b` at `x = 1` and `−1 + b` at `x = −1`. The gap is
    /// exactly `a`, the constant the polynomial adds to keep its output averaging around zero, so
    /// the clipping cases read as having been written before that constant was included. Taken
    /// literally the curve steps by 0.125 at `|x| = 1`, which is a click generator and fails this
    /// collection's requirement that every saturator be monotonic. **The clamp is therefore taken
    /// at the polynomial's own endpoints**, which is the reading that makes the paper's own sentence
    /// true.
    pub const UPPER: f32 = 1.0 - B;
    /// The lower bound, exactly. See [`UPPER`].
    pub const LOWER: f32 = -1.0 + B;

    /// The paper's curve. Bounded by [`UPPER`] and [`LOWER`], and monotonic across the whole real
    /// line: `f'(x) = 1 − 2a·x − 3b·x²` has its roots at `x = 1.81` and `x = −3.31`, both outside
    /// the interval where the formula is used.
    #[inline]
    pub fn curve(x: f32) -> f32 {
        if x > 1.0 {
            UPPER
        } else if x < -1.0 {
            LOWER
        } else {
            x - A * x * x - B * x * x * x + A
        }
    }
}

/// How far the bias trimmer multiplies the distortion at either end of its travel.
///
/// **Read off the catalogue's own curve** (THD — V_Bias for the MN3005, p. 20): a U with a sharp
/// minimum near −8.2 V reading about 0.35 %, against 1.2 % at −7 V and 1.6 % at −9.5 V — three to
/// five times more distorted at the same level. Four is the middle of what that curve shows.
pub const BIAS_THD_MULTIPLIER: f32 = 4.0;

/// The DC offset a fully mis-trimmed bias puts into the nonlinearity.
///
/// **Chosen.** The catalogue specifies the bias pin only as a *range* with the instruction *"adjust
/// so as to obtain the minimum distortion"*, so there is no curve to read a number off. Large
/// enough that the asymmetry is audible as the second harmonic the trimmer is there to null, small
/// enough that a full-scale signal at the dirtiest line still lands inside the polynomial rather
/// than on its clamp. What would replace it: the output spectrum of a real part with the trimmer
/// swept, which the research page already lists as one of the three things worth measuring.
pub const BIAS_OFFSET: f32 = 0.08;

/// The noise's corner, as a fraction of the clock.
///
/// **Chosen**, and chosen to keep the thesis intact: the noise arises *in* the line, so it is
/// clocked like everything else, and its colour narrows as the clock slows. A fixed-frequency hiss
/// would be a hiss generator bolted to a delay; this is the device's own floor, which is why it
/// darkens when the delay lengthens. The research page's *"low-amplitude coloured noise"* is all
/// the sources say about the colour.
pub const NOISE_COLOUR_RATIO: f32 = 0.1;

/// Below this total fader weight the mixer has nothing open and its output is silence.
///
/// A floor rather than a test against zero, because the mixer divides by the weight: a single fader
/// at a millionth would otherwise be normalised back to full scale and read as a click.
pub const TAP_WEIGHT_FLOOR: f32 = 1e-3;

/// The level the distortion figures are quoted against, as a fraction of full scale.
///
/// **Chosen**: −6 dBFS, an ordinary operating level for an effect that expects instrument or line
/// signal. The catalogue's THD figures are quoted at the part's own maximum input voltage, which
/// has no digital equivalent, so the reference level has to be picked and stated.
pub const THD_REFERENCE_LEVEL: f32 = 0.5;

/// Which bucket brigade is fitted.
///
/// The switch is four-way *because the hardware is*: these are four different chips, and picking
/// one sets length, noise floor, bandwidth and distortion **together**. Two lines at the same delay
/// time sound different, which is the point of the control and the reason it is not a "size" knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    /// MN3007, 1024 stages.
    S1024,
    /// MN3011, 3328 stages — the six-tap part, and the one whose tap constellation every other
    /// line borrows.
    S3328,
    /// MN3005, 4096 stages.
    S4096,
    /// Two MN3005s, 8192 stages: the pair one MN3101 is specified to drive.
    S8192,
}

impl Line {
    pub const ALL: [Line; 4] = [Line::S1024, Line::S3328, Line::S4096, Line::S8192];

    #[inline]
    pub const fn stages(self) -> usize {
        match self {
            Line::S1024 => 1024,
            Line::S3328 => 3328,
            Line::S4096 => 4096,
            Line::S8192 => 8192,
        }
    }

    /// **The shortest and longest delay this chip can be clocked to**, in seconds.
    ///
    /// Straight from the delay law and the clock's own limits: `t = N / (2 f)`, so
    /// `[N / 2f_max, N / 2f_min]`. It is a property of the *fitted part* and not of the product's
    /// taste — the 1024-stage line cannot reach three quarters of a second at any clock the MN3101
    /// generates, whatever a control asks for.
    ///
    /// The parameter layer needs this to answer a question the DSP cannot: **which tempo
    /// subdivisions are actually reachable**. Without it a synced delay offers a division the chip
    /// then silently clamps, and the product lies about being in time.
    #[inline]
    pub fn delay_bounds(self) -> (f32, f32) {
        let stages = self.stages() as f64;
        let per_period = STAGES_PER_CLOCK_PERIOD as f64;
        (
            (stages / (per_period * crate::CLOCK_MAX_HZ)) as f32,
            (stages / (per_period * crate::CLOCK_MIN_HZ)) as f32,
        )
    }

    /// Typical total harmonic distortion, as a fraction — the catalogue's quick reference table
    /// (p. 2), except for the 8192 pair.
    ///
    /// **The order is the catalogue's and is not monotonic**, which matters because it would be
    /// easy to "fix": the MN3011 is a *low noise* part specified at 0.4 % where the shorter MN3007
    /// is specified at 0.5 %. Longer is dirtier across the family as a whole — 0.2 % at 128 stages
    /// to 1 % at 4096 — and the six-tap part is the exception the manufacturer built.
    ///
    /// The 8192 figure is **computed, not read**: two MN3005s in series, each contributing its own
    /// 1 %, and no row of the catalogue describes the pair.
    #[inline]
    pub const fn thd_typical(self) -> f32 {
        match self {
            Line::S1024 => 0.005,
            Line::S3328 => 0.004,
            Line::S4096 => 0.010,
            Line::S8192 => 0.020,
        }
    }

    /// Typical signal-to-noise ratio in dB — the catalogue's table (p. 2), and **this one is
    /// monotonic**: 80 dB at 1024 stages, 76 at 3328, 75 at 4096, about 3 dB per doubling, which
    /// the catalogue also prints as its own *S/N — BBD STAGE* curve (p. 7).
    ///
    /// The 8192 figure is **computed**: two MN3005s, so 3 dB worse than one.
    #[inline]
    pub const fn signal_to_noise_db(self) -> f32 {
        match self {
            Line::S1024 => 80.0,
            Line::S3328 => 76.0,
            Line::S4096 => 75.0,
            Line::S8192 => 72.0,
        }
    }

    /// The drive into the polynomial that produces this part's own distortion figure at
    /// [`THD_REFERENCE_LEVEL`].
    ///
    /// The polynomial's second-harmonic term is `a·x²/2` against a fundamental of `x`, so
    /// `THD ≈ a·g·A/2` and the drive that hits a given figure is `2·THD / (a·A)`. **Raffel & Smith's
    /// own THD law is deliberately not used**: `1.01^(N/1024) − 1` predicts about 4 % at 4096 stages
    /// where Panasonic's catalogue prints 1 % typical and 2.5 % maximum. The law's *shape* —
    /// distortion compounding with stage count — is right and is the useful part; its constant is
    /// not established, and adopting it would bake a fourfold error into the product's most audible
    /// quantity.
    #[inline]
    pub fn distortion_drive(self) -> f32 {
        2.0 * self.thd_typical() / (nonlinearity::A * THD_REFERENCE_LEVEL)
    }

    /// The loop gain at which this line, already running, keeps going instead of decaying.
    ///
    /// **Measured, by `examples/feedback_spike.rs`** — not derived, and it could not be: the
    /// compander is inside the loop, so its compressor lifts a decaying tail a little on every lap
    /// and the line holds at a lower gain than the takeoff sum alone would say. A longer line holds
    /// at a lower gain than a short one, because it is noisier and its own floor keeps feeding it.
    ///
    /// **This is the *running* threshold, and calibrating to the other one is the trap.**
    /// `mxm-folded-spring` cost three attempts on exactly this: a line sitting in silence has
    /// snapped to zero, so from silence it needs more gain to start than a running one needs to
    /// hold. A delay somebody is playing through is never silent, so the running figure is the one
    /// a player meets, and the control is mapped against it.
    ///
    /// It lives here rather than in the plugin's parameter layer — where the spring keeps its
    /// equivalent — because the DSP needs it too: the snap to exact zero must not fire while the
    /// loop is in the self-sustaining region, or a line could never sing from its own noise the way
    /// the hardware does.
    #[inline]
    pub const fn sings_at(self) -> f32 {
        match self {
            Line::S1024 => 1.0371,
            Line::S3328 => 0.9397,
            Line::S4096 => 0.9126,
            Line::S8192 => 0.8730,
        }
    }

    /// The loop gain at which this line **starts** from silence, on its own noise, the way the
    /// hardware does.
    ///
    /// Measured by the same facility, and it is a quarter to a half above [`Self::sings_at`]. That
    /// gap is the whole reason the calibration is delicate: a line that has snapped to zero has no
    /// seed but its own floor, so it needs more gain to get going than a running one needs to keep
    /// going. Full travel on the control reaches past this for every line, which is what lets a
    /// parked, silent instance be woken by `Feedback` alone.
    #[inline]
    pub const fn starts_at(self) -> f32 {
        match self {
            Line::S1024 => 1.2942,
            Line::S3328 => 1.2744,
            Line::S4096 => 1.2678,
            Line::S8192 => 1.2598,
        }
    }

    /// The tap positions for this line, in **stages**: the MN3011's constellation scaled to this
    /// length, then stretched about the last tap by `spread` (1.0 is Panasonic's ratios).
    ///
    /// Stages, not clock periods — see [`STAGES_PER_CLOCK_PERIOD`] for the conversion, which is
    /// where the delay law's factor of two lives.
    pub fn tap_stages(self, spread: f32) -> [usize; TAPS] {
        let n = self.stages();
        let mut out = [0usize; TAPS];
        for (i, &s) in MN3011_TAP_STAGES.iter().enumerate() {
            let ratio = s as f32 / MN3011_STAGES as f32;
            // Stretch about tap 6, which sits at the end of the line and does not move: at
            // `spread = 0` every tap collapses onto it and the part is an ordinary single delay.
            let stretched = 1.0 - spread * (1.0 - ratio);
            let stages = (stretched * n as f32).round() as i64;
            out[i] = stages.clamp(1, n as i64) as usize;
        }
        out
    }
}

/// Where on the control's travel every line begins to sing.
///
/// The last tenth, which is what the plan asks for: high enough that the usable range is nearly all
/// of the control, low enough that the singing region is reachable and holdable rather than a
/// hair's width at the very top. `mxm-folded-spring`'s figure, and there is no reason for two
/// products in one collection to disagree about where a feedback control sings.
pub const SINGS_AT_CONTROL: f32 = 0.9;

/// How far past its own sustain threshold full travel drives the loop.
///
/// **Chosen against a measurement**: the from-silence threshold is between 1.25 and 1.45 times the
/// running one across the four lines ([`Line::starts_at`] over [`Line::sings_at`]), so full travel
/// has to clear 1.45 for a parked instance to be startable by `Feedback` alone. One and a half
/// clears every line with a little to spare and nothing to waste.
pub const FULL_TRAVEL_MULTIPLE: f32 = 1.5;

/// The control curve: travel in `0..1` to the loop gain the line runs at.
///
/// Two segments, and the join is where it sings:
///
/// - **Below [`SINGS_AT_CONTROL`]** the travel is *squared*, which puts the useful range — the
///   repeats a player actually sets — across most of the sweep instead of crowding it at the top.
/// - **Above it** the curve runs on to [`FULL_TRAVEL_MULTIPLE`] of the threshold at full travel, so
///   the last tenth is the singing region and the very top starts a line from silence.
///
/// Every line sings at the same place on the control because the curve is scaled by that line's own
/// measured threshold. Without it, one constant would put the singing point somewhere different on
/// each of the four — which is precisely the defect `mxm-folded-spring` shipped and had to fix.
pub fn feedback_gain(line: Line, travel: f32) -> f32 {
    let travel = travel.clamp(0.0, 1.0);
    let u = travel / SINGS_AT_CONTROL;
    let shape = if u <= 1.0 {
        u * u
    } else {
        let slope = (FULL_TRAVEL_MULTIPLE - 1.0) / (1.0 / SINGS_AT_CONTROL - 1.0);
        1.0 + slope * (u - 1.0)
    };
    line.sings_at() * shape
}

/// Tap positions in stages, as the clock periods the line is actually indexed by.
///
/// A sample crosses [`STAGES_PER_CLOCK_PERIOD`] stages per period, so a tap `s` stages along the
/// line comes back `s / 2` periods later — which is the delay law, applied per tap.
fn ticks_from_stages(stages: [usize; TAPS]) -> [usize; TAPS] {
    let mut out = [0usize; TAPS];
    for i in 0..TAPS {
        // Rounded, not truncated: an odd stage count is half a period from either neighbour and
        // truncation would always bias the constellation early.
        let half = STAGES_PER_CLOCK_PERIOD / 2;
        out[i] = ((stages[i] + half) / STAGES_PER_CLOCK_PERIOD).max(1);
    }
    out
}

/// Which filters are fitted around the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    /// Sallen-Key at the hardware's own 2 kHz, chosen for the longest delay the circuit reaches —
    /// so short delays stay dark, because the filter does not know how fast the clock is running.
    /// This is Panasonic's own choice in both reference echo circuits.
    Fixed,
    /// Switched-capacitor, clocked from the same clock as the line, so the cutoff follows the delay
    /// and short delays keep their top end. A minority of real designs do this; it is a choice the
    /// hardware made, not an invention.
    Tracking,
}

/// One bucket brigade device and the filters either side of it.
///
/// Allocated once, at activation, at [`MAX_STAGES`]; nothing here allocates afterwards, so a `Line`
/// change is an index change under a fade rather than a new buffer.
#[derive(Debug, Clone)]
pub struct Bbd {
    line: Line,
    stages: usize,
    buf: Vec<f32>,
    write: usize,

    anti_alias: ModalLowpass,
    recon_mix: ModalLowpass,
    recon_tail: ModalLowpass,

    /// The sample-and-hold: what the line is presenting to the reconstruction filter until the next
    /// clock tick. Two of them, because `Return = Tail` feeds the loop the last tap alone while the
    /// output still carries the mix, and one post-mix chain cannot carry both at once.
    hold_mix: f64,
    hold_tail: f64,

    /// Seconds until the next clock tick. Carried across host samples, which is what lets the clock
    /// be anything at all relative to the host rate.
    until_tick: f64,

    /// Tap positions as **clock periods back**, which is what the buffer is indexed in.
    taps: [usize; TAPS],
    tap_gains: [f32; TAPS],
    spread: f32,

    noise: Rng,
    noise_state: f32,
    noise_level: f32,

    bias: f32,
    drive: f32,
    filter_mode: FilterMode,
}

impl Bbd {
    /// A line with its filters, ready at `_sample_rate`. Allocates; call from the main thread at
    /// activation.
    pub fn new(line: Line, seed: u32) -> Self {
        let mut bbd = Self {
            line,
            stages: line.stages(),
            buf: vec![0.0; BUFFER_SLOTS],
            write: 0,
            anti_alias: ModalLowpass::butterworth(&[3], FIXED_CUTOFF_HZ),
            recon_mix: ModalLowpass::butterworth(&[3, 2], FIXED_CUTOFF_HZ),
            recon_tail: ModalLowpass::butterworth(&[3, 2], FIXED_CUTOFF_HZ),
            hold_mix: 0.0,
            hold_tail: 0.0,
            until_tick: 0.0,
            taps: [0; TAPS],
            tap_gains: [1.0; TAPS],
            spread: 1.0,
            noise: Rng::new(seed),
            noise_state: 0.0,
            noise_level: 0.0,
            bias: 0.0,
            drive: 0.0,
            filter_mode: FilterMode::Fixed,
        };
        bbd.set_line(line);
        bbd
    }

    /// Fit a different chip. Length, noise floor and distortion all change together — that is the
    /// control's whole meaning — and the line's contents are *not* rescaled: a `Line` change is
    /// audibly a new delay, not a stretched one, because it is a different part. The caller fades
    /// the wet around it (the plan's §4).
    pub fn set_line(&mut self, line: Line) {
        self.line = line;
        self.stages = line.stages();
        self.drive = line.distortion_drive();
        // The catalogue quotes S/N at the device's output; the noise is injected where the chip
        // makes it, at the line's input, so what reaches the output is this level shaped by
        // everything after it. The *ordering* across the four parts, which is the product's whole
        // premise, is what that preserves.
        self.noise_level = 10f32.powf(-line.signal_to_noise_db() / 20.0);
        self.taps = ticks_from_stages(line.tap_stages(self.spread));
        self.clear();
    }

    #[inline]
    pub fn line(&self) -> Line {
        self.line
    }

    /// Stretch the tap constellation about the last tap. 1.0 is Panasonic's ratios.
    pub fn set_spread(&mut self, spread: f32) {
        let spread = spread.clamp(0.0, 2.0);
        if spread != self.spread {
            self.spread = spread;
            self.taps = ticks_from_stages(self.line.tap_stages(spread));
        }
    }

    /// The six fader gains, as **relative** weights. This is the mixing ladder opened up: the
    /// reference reverberation circuit's own 100–150 kΩ ladder is a 3.5 dB tilt across the set, and
    /// it lives in a preset rather than in here, because with the faders exposed the faders *are*
    /// the ladder.
    ///
    /// **Relative, not absolute** — see [`Bbd::tick`]'s weighted average. The faders decide the
    /// *shape* of the constellation; `Mix` decides how much of it reaches the output and
    /// `Feedback` how much of it goes round again. Each control then means one thing, which is what
    /// the owner could not find in the first version: there, opening a fader made the wash louder
    /// *and* hotter at once, so no single knob had a stable meaning.
    #[inline]
    pub fn set_tap_gains(&mut self, gains: [f32; TAPS]) {
        self.tap_gains = gains;
    }

    /// Where the bias trimmer is set: 0.0 is the distortion minimum every unit left the factory
    /// trimmed to, ±1.0 the ends of its travel.
    #[inline]
    pub fn set_bias(&mut self, bias: f32) {
        self.bias = bias.clamp(-1.0, 1.0);
    }

    #[inline]
    pub fn set_filter_mode(&mut self, mode: FilterMode) {
        self.filter_mode = mode;
    }

    /// Clear everything the line holds. Used by `reset()` and by every transition that brings a
    /// line into use, because no transition preserves an outgoing tail.
    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
        self.write = 0;
        self.hold_mix = 0.0;
        self.hold_tail = 0.0;
        self.until_tick = 0.0;
        self.noise_state = 0.0;
        self.anti_alias.reset();
        self.recon_mix.reset();
        self.recon_tail.reset();
    }

    /// The device's distortion at the current bias: the polynomial, driven to the part's own THD
    /// figure and offset by the trimmer.
    ///
    /// The offset's own DC is subtracted because the circuit is AC-coupled — a mis-trimmed BBD
    /// distorts, it does not add a bias voltage to the audio.
    #[inline]
    fn distort(&self, x: f32) -> f32 {
        let g = self.drive * (1.0 + (BIAS_THD_MULTIPLIER - 1.0) * self.bias.abs());
        let offset = BIAS_OFFSET * self.bias;
        // **The clamp is on the input, at the device's own full scale.** Every part in the
        // catalogue specifies a `V_i(max)` — 1.2 V for the MN3005 — and it clips there, at its
        // input, whatever its distortion figure happens to be. Scaling the drive to hit a part's
        // THD and then letting the *clamp* move with it would make the dirtiest line the one with
        // the most headroom, which is backwards, and would leave the line's output bound
        // proportional to `1/g` — six laps of a loop later, that is how a model blows up.
        let xc = x.clamp(-1.0, 1.0);
        (nonlinearity::curve(g * xc + offset) - nonlinearity::curve(offset)) / g
    }

    /// What the line can put out, whatever goes in. Bounded by construction: the input is clamped
    /// at full scale and the curve is monotonic, so the extreme is the curve at the extreme.
    pub fn output_bound(&self) -> f32 {
        let g = self.drive * (1.0 + (BIAS_THD_MULTIPLIER - 1.0) * self.bias.abs());
        let offset = BIAS_OFFSET * self.bias;
        let hi = (nonlinearity::curve(g + offset) - nonlinearity::curve(offset)) / g;
        let lo = (nonlinearity::curve(-g + offset) - nonlinearity::curve(offset)) / g;
        hi.abs().max(lo.abs())
    }

    /// One clock tick: sample the anti-alias filter, add the chip's noise, distort, write, and read
    /// the taps into the two holds.
    #[inline]
    fn tick(&mut self, clock: Clock) {
        let sampled = self.anti_alias.output() as f32;

        // The chip's own floor, at the chip's own input, clocked like everything else: a one-pole
        // on white noise whose corner is a fraction of the clock, so the colour narrows as the
        // delay lengthens.
        let white = self.noise.next_bipolar();
        let coeff = NOISE_COLOUR_RATIO.clamp(0.0, 0.5);
        self.noise_state += coeff * (white - self.noise_state);
        let with_noise = sampled + self.noise_level * self.noise_state;

        let written = flush(self.distort(with_noise));
        self.buf[self.write] = written;

        // **Normalised by the root of the sum of squares, not by the sum.** The faders set the
        // constellation's *shape*; the mixer's output level is the faders' business no more than
        // the line's length is.
        //
        // Three normalisers, and why this one:
        //
        // - **Summing** (what shipped first) multiplies the level by the number of open faders, so
        //   `Mix` and `Feedback` both move when a fader does and neither knob has a stable
        //   meaning. That is the defect this replaced.
        // - **Averaging** divides by the sum, which is right for *correlated* taps and wrong for
        //   these: six returns at the MN3011's deliberately non-multiple spacings are decorrelated,
        //   so their average comes out at `1/√6` of one tap. Measured at 0.42 against 1.03.
        // - **Root-sum-square** holds the level constant for decorrelated content, which is what
        //   the constellation is built to be. `√Σg²` is one for a single fader at unity, so the
        //   Init patch is untouched.
        let mut mix = 0.0f32;
        let mut power = 0.0f32;
        let mut weight = 0.0f32;
        let longest = self.stages / STAGES_PER_CLOCK_PERIOD;
        for i in 0..TAPS {
            let back = self.taps[i].clamp(1, longest);
            let idx = (self.write + BUFFER_SLOTS - back) % BUFFER_SLOTS;
            let s = self.buf[idx];
            mix += self.tap_gains[i] * s;
            power += self.tap_gains[i] * self.tap_gains[i];
            weight += self.tap_gains[i].abs();
            if i == TAPS - 1 {
                // The takeoff under `Return = Tail` is the last tap **as the line holds it**: that
                // mode means *feed the loop the last tap*, and scaling it by a fader that is only a
                // relative weight would make the same control mean two things.
                self.hold_tail = flush(s) as f64;
            }
        }
        // **Every fader down closes the mixer, and that closes the loop too.** Without the second
        // half, `Return = Tail` would go on feeding the line its own last tap while the output was
        // silent: the delay would sing to itself unheard, and raising one fader would produce a
        // wash already howling. All the way down means no delay.
        if weight > TAP_WEIGHT_FLOOR {
            self.hold_mix = flush(mix / power.sqrt()) as f64;
        } else {
            self.hold_mix = 0.0;
            self.hold_tail = 0.0;
        }
        self.write = (self.write + 1) % BUFFER_SLOTS;

        let _ = clock;
    }

    /// Advance one host sample.
    ///
    /// `input` is held for the whole sample, which is what the host's own sampling already did to
    /// it. Returns `(mix, tail)`: the tap mix that goes to the output, and the last tap alone,
    /// which is what `Return = Tail` feeds back. Both have been through their own reconstruction
    /// filter — two branches, because one post-mix chain cannot carry the six-tap mix to the output
    /// and tap 6 to the loop at the same time.
    #[inline]
    pub fn process(
        &mut self,
        input: f32,
        clock: Clock,
        dt_host: f64,
        need_tail: bool,
    ) -> (f32, f32) {
        match self.filter_mode {
            FilterMode::Fixed => {
                self.anti_alias.set_cutoff(FIXED_CUTOFF_HZ);
                self.recon_mix.set_cutoff(FIXED_CUTOFF_HZ);
                if need_tail {
                    self.recon_tail.set_cutoff(FIXED_CUTOFF_HZ);
                }
            }
            FilterMode::Tracking => {
                let f = clock.hz() * TRACKING_CUTOFF_RATIO;
                self.anti_alias.set_cutoff(f);
                self.recon_mix.set_cutoff(f);
                if need_tail {
                    self.recon_tail.set_cutoff(f);
                }
            }
        }

        let period = clock.period();
        // The anti-alias filter's state is recursive and not flushed: a non-finite sample held
        // there would be written into the line on every tick.
        let u = finite_or_zero(input) as f64;
        let mut remaining = dt_host;

        // Walk the ticks that fall inside this host sample. The bound is arithmetic rather than a
        // hope: the clock is clamped and the host rate is not zero, so the count is finite, and the
        // guard is there for the pathological case rather than the ordinary one.
        let mut guard = 0;
        while self.until_tick < remaining && guard < 64 {
            let step = self.until_tick;
            self.anti_alias.advance(step, u);
            self.recon_mix.advance(step, self.hold_mix);
            if need_tail {
                self.recon_tail.advance(step, self.hold_tail);
            }
            self.tick(clock);
            remaining -= step;
            self.until_tick = period;
            guard += 1;
        }

        self.anti_alias.advance(remaining, u);
        self.recon_mix.advance(remaining, self.hold_mix);
        if need_tail {
            self.recon_tail.advance(remaining, self.hold_tail);
        }
        self.until_tick -= remaining;

        let mix = flush(self.recon_mix.output() as f32);
        let tail = if need_tail {
            flush(self.recon_tail.output() as f32)
        } else {
            mix
        };
        (mix, tail)
    }

    /// The anti-alias filter's magnitude at `hz`, for tests that divide the filters out of a
    /// measured roll-off to see the hold on its own.
    pub fn filter_magnitude(&self, hz: f64) -> f64 {
        self.anti_alias.magnitude(hz) * self.recon_mix.magnitude(hz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 192_000.0;

    fn open_taps(bbd: &mut Bbd) {
        bbd.set_tap_gains([1.0; TAPS]);
    }

    /// Only the last tap, which is the plain single delay every echo circuit uses.
    fn only_last_tap(bbd: &mut Bbd) {
        let mut g = [0.0; TAPS];
        g[TAPS - 1] = 1.0;
        bbd.set_tap_gains(g);
    }

    /// Run an impulse through and return the output.
    fn impulse_response(bbd: &mut Bbd, clock: Clock, samples: usize) -> Vec<f32> {
        let dt = 1.0 / FS;
        (0..samples)
            .map(|i| {
                let x = if i == 0 { 1.0 } else { 0.0 };
                bbd.process(x, clock, dt, false).0
            })
            .collect()
    }

    /// The sample index of the largest magnitude in a window.
    fn peak_index(y: &[f32], from: usize, to: usize) -> usize {
        let mut best = from;
        for i in from..to.min(y.len()) {
            if y[i].abs() > y[best].abs() {
                best = i;
            }
        }
        best
    }

    /// Amplitude at one frequency, by a single-bin DFT over a whole number of periods. Isolates the
    /// fundamental from the images the sampling puts either side of the clock, which a peak
    /// measurement cannot do.
    fn amplitude_at(y: &[f32], hz: f64, fs: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        let n = y.len();
        for (i, &s) in y.iter().enumerate() {
            let t = core::f64::consts::TAU * hz * (i as f64) / fs;
            re += s as f64 * t.cos();
            im -= s as f64 * t.sin();
        }
        2.0 * (re * re + im * im).sqrt() / n as f64
    }

    /// The bounds are the clock's limits put through the delay law, and they are **ordered, distinct
    /// and monotonic in stage count** — a longer chip reaches both further and no shorter. The
    /// parameter layer decides which subdivisions are reachable from these, so a wrong one here is a
    /// synced delay that is quietly out of time.
    #[test]
    fn each_line_reaches_from_its_fastest_clock_to_its_slowest() {
        let mut previous: Option<(f32, f32)> = None;
        for line in Line::ALL {
            let (short, long) = line.delay_bounds();
            assert!(short > 0.0 && short < long, "{line:?}: {short} to {long}");
            // The law itself, at both ends.
            let n = line.stages() as f32;
            assert!((short - n / (2.0 * crate::CLOCK_MAX_HZ as f32)).abs() < 1e-9);
            assert!((long - n / (2.0 * crate::CLOCK_MIN_HZ as f32)).abs() < 1e-6);
            if let Some((was_short, was_long)) = previous {
                assert!(
                    short > was_short && long > was_long,
                    "{line:?} is not longer"
                );
            }
            previous = Some((short, long));
        }
    }

    /// **The delay law, through the actual line** — not through the arithmetic, which `lib.rs`
    /// already checks against Panasonic's reference circuits. An impulse goes in, the last tap's
    /// return is timed, and it has to land where `N / (2 · f_CP)` says.
    #[test]
    fn the_measured_delay_is_the_stage_count_over_twice_the_clock() {
        for line in Line::ALL {
            for &ms in &[40.0, 150.0, 400.0] {
                let clock = Clock::for_delay(line.stages(), ms / 1000.0);
                let mut bbd = Bbd::new(line, 1);
                only_last_tap(&mut bbd);
                // Tracking keeps the filters wide enough that the impulse stays a sharp blob; the
                // fixed 2 kHz filter would smear it over milliseconds and blunt the measurement
                // without changing where the peak is.
                bbd.set_filter_mode(FilterMode::Tracking);

                let expected = clock.delay_seconds(line.stages());
                let n = ((expected * 1.5 + 0.01) * FS) as usize;
                let y = impulse_response(&mut bbd, clock, n);
                let peak = peak_index(&y, 1, n) as f64 / FS;

                // The filters are in the path and they delay it: third order in, third plus second
                // out, whose group delay at the tracking cutoff comes to about two and a half clock
                // periods. That is the circuit's own and not an error — a real echo circuit's
                // measured delay includes its filters too — so the return must land *late* by a
                // couple of periods and never early, and never by more than the filters can
                // account for. The exact claim the filters cannot touch is the tap *spacing*, which
                // `the_six_taps_land_at_the_mn3011s_proportions` measures.
                let slip = (peak - expected) / clock.period();
                assert!(
                    (0.0..5.0).contains(&slip),
                    "{line:?} at {ms} ms: measured {peak:.4} s against the law's {expected:.4} s,                      which is {slip:.2} clock periods of slip"
                );
            }
        }
    }

    /// **The taps land where the device puts them.** One impulse, six returns, each at the MN3011's
    /// own proportion of the line — and the proportions are not multiples of each other, so a model
    /// that had quietly spaced them evenly would fail on tap 2 alone.
    #[test]
    fn the_six_taps_land_at_the_mn3011s_proportions() {
        for line in Line::ALL {
            let clock = Clock::for_delay(line.stages(), 0.300);
            let stages = line.tap_stages(1.0);
            let mut measured = [0.0f64; TAPS];

            for tap in 0..TAPS {
                let mut bbd = Bbd::new(line, 1);
                bbd.set_filter_mode(FilterMode::Tracking);
                let mut g = [0.0; TAPS];
                g[tap] = 1.0;
                bbd.set_tap_gains(g);

                let ratio = MN3011_TAP_STAGES[tap] as f64 / MN3011_STAGES as f64;
                let ideal = clock.delay_seconds(line.stages()) * ratio;
                let n = ((ideal * 1.6 + 0.02) * FS) as usize;
                let y = impulse_response(&mut bbd, clock, n);
                measured[tap] = peak_index(&y, 1, n) as f64 / FS;

                // Each tap carries the same filter group delay as the whole line does, so the
                // absolute position is bounded rather than exact — see the delay-law test.
                let slip = (measured[tap] - ideal) / clock.period();
                assert!(
                    (0.0..5.0).contains(&slip),
                    "{line:?} tap {}: measured {:.4} s against the device's {ideal:.4} s,                      which is {slip:.2} clock periods of slip",
                    tap + 1,
                    measured[tap]
                );
            }

            // **The spacing is the line alone.** Every tap is delayed equally by the filters, so
            // the differences between them are the constellation itself, and they are exact. This
            // is what would catch a model that had quietly spaced the taps evenly: not one of the
            // MN3011's positions is a multiple of another, so an even spacing survives no
            // difference here.
            for tap in 1..TAPS {
                let want = (ticks_from_stages(stages)[tap] as f64
                    - ticks_from_stages(stages)[0] as f64)
                    * clock.period();
                let got = measured[tap] - measured[0];
                assert!(
                    (got - want).abs() < 3.0 / FS,
                    "{line:?} tap {} sits {got:.5} s after tap 1, the line says {want:.5} s",
                    tap + 1
                );
            }
        }
    }

    /// All six at once, from one impulse, in order and distinct. The single-tap test above is the
    /// precise one; this is the claim as the plan states it.
    #[test]
    fn one_impulse_returns_six_times() {
        let line = Line::S3328;
        let clock = Clock::for_delay(line.stages(), 0.300);
        let mut bbd = Bbd::new(line, 1);
        bbd.set_filter_mode(FilterMode::Tracking);
        open_taps(&mut bbd);
        let y = impulse_response(&mut bbd, clock, (0.35 * FS) as usize);

        let mut last = 0.0;
        for (tap, &tap_stages) in MN3011_TAP_STAGES.iter().enumerate() {
            let ratio = tap_stages as f64 / MN3011_STAGES as f64;
            let at = clock.delay_seconds(line.stages()) * ratio;
            let centre = (at * FS) as usize;
            let half = (0.0015 * FS) as usize;
            let peak = peak_index(&y, centre - half, centre + half);
            let level = y[peak].abs();
            assert!(level > 1e-4, "tap {} is missing (peak {level:e})", tap + 1);
            let at_measured = peak as f64 / FS;
            assert!(at_measured > last, "tap {} is out of order", tap + 1);
            last = at_measured;
        }
    }

    /// **Turning Time bends pitch**, and in the right direction — the half a wrong implementation
    /// still gets to fifty-fifty. A tone is held while the clock is swept *slower*; the samples
    /// already in the line then leave at the new rate, so the pitch must fall.
    #[test]
    fn slowing_the_clock_bends_the_tail_down_and_speeding_it_bends_up() {
        for (name, from_ms, to_ms) in [("slower", 100.0, 200.0), ("faster", 200.0, 100.0)] {
            let line = Line::S4096;
            let mut bbd = Bbd::new(line, 1);
            bbd.set_filter_mode(FilterMode::Tracking);
            only_last_tap(&mut bbd);

            let tone = 400.0;
            let dt = 1.0 / FS;
            let fill = (0.5 * FS) as usize;
            for i in 0..fill {
                let t = i as f64 * dt;
                let x = (core::f64::consts::TAU * tone * t).sin() as f32;
                bbd.process(
                    x,
                    Clock::for_delay(line.stages(), from_ms / 1000.0),
                    dt,
                    false,
                );
            }

            // Now sweep the clock while the input is silent: what comes out is what was already in
            // the line, leaving at the new rate.
            let sweep = (0.08 * FS) as usize;
            let mut out = Vec::with_capacity(sweep);
            for i in 0..sweep {
                let k = i as f64 / sweep as f64;
                let ms = from_ms + (to_ms - from_ms) * k;
                let clock = Clock::for_delay(line.stages(), ms / 1000.0);
                out.push(bbd.process(0.0, clock, dt, false).0);
            }

            // Zero crossings measure the pitch without needing a transform: count them over the
            // first and last thirds of the sweep.
            let crossings = |s: &[f32]| {
                s.windows(2)
                    .filter(|w| (w[0] <= 0.0) != (w[1] <= 0.0))
                    .count()
            };
            let third = out.len() / 3;
            let early = crossings(&out[..third]);
            let late = crossings(&out[2 * third..]);
            if name == "slower" {
                assert!(
                    late < early,
                    "slowing must lower the pitch: {early} then {late}"
                );
            } else {
                assert!(
                    late > early,
                    "speeding must raise the pitch: {early} then {late}"
                );
            }
        }
    }

    /// **The output hold is there.** Holding each sample for one clock period convolves the output
    /// with a rectangle of width `T`, so the response carries a sinc: −3.92 dB at the line's own
    /// Nyquist, −1.33 dB at Panasonic's stated input bandwidth limit of `0.3 · f_CP`. The filters
    /// are divided out analytically, so what is left is the hold alone — if our line does not show
    /// it, the hold is missing and the datasheets' unexplained insertion-gain roll-off would have
    /// to be faked with a fitted curve instead.
    #[test]
    fn the_sample_and_hold_puts_a_sinc_on_the_output() {
        let line = Line::S4096;
        let clock = Clock::from_hz(20_000.0);
        let mut bbd = Bbd::new(line, 1);
        only_last_tap(&mut bbd);
        // Wide filters: the sinc is what is being measured, and a 2 kHz filter would leave nothing
        // to measure at 9 kHz. The filters are divided out below in any case.
        bbd.set_filter_mode(FilterMode::Tracking);
        // Silence the chip's own noise so the measurement is of the hold and nothing else.
        bbd.noise_level = 0.0;

        let dt = 1.0 / FS;
        let settle = (0.25 * FS) as usize;
        let measure = (0.25 * FS) as usize;

        for &(hz, expected_db) in &[
            (2000.0, -0.1434), // sinc at 0.1 · f_CP
            (6000.0, -1.3260), // sinc at 0.3 · f_CP — Panasonic's own stated f_i limit
            (9000.0, -3.1128), // sinc at 0.45 · f_CP, as close to Nyquist as the images allow
        ] {
            let mut b = bbd.clone();
            for i in 0..settle {
                let t = i as f64 * dt;
                b.process(
                    (core::f64::consts::TAU * hz * t).sin() as f32,
                    clock,
                    dt,
                    false,
                );
            }
            let out: Vec<f32> = (0..measure)
                .map(|i| {
                    let t = (settle + i) as f64 * dt;
                    b.process(
                        (core::f64::consts::TAU * hz * t).sin() as f32,
                        clock,
                        dt,
                        false,
                    )
                    .0
                })
                .collect();

            let measured = amplitude_at(&out, hz, FS);
            // Divide out the two filters, which are known analytically, leaving the hold.
            let hold_only = measured / b.filter_magnitude(hz);
            let db = 20.0 * hold_only.log10();
            // The sinc the hold is supposed to be.
            let x = core::f64::consts::PI * hz / clock.hz();
            let sinc_db = 20.0 * (x.sin() / x).log10();
            assert!(
                (sinc_db - expected_db).abs() < 0.02,
                "the reference sinc itself moved at {hz} Hz: {sinc_db}"
            );
            assert!(
                (db - sinc_db).abs() < 0.35,
                "{hz} Hz: measured {db:.2} dB after dividing the filters out, sinc says {sinc_db:.2} dB"
            );
        }
    }

    /// The hold's roll-off at the line's own Nyquist is the number the research page computes and
    /// Raffel & Smith's bench measurement lands on: −3.92 dB. Asserted on the model's own sinc so
    /// the constant cannot drift away from the document.
    #[test]
    fn the_holds_roll_off_at_nyquist_is_the_computed_minus_3_92_db() {
        let x = core::f64::consts::PI * 0.5;
        let db = 20.0 * (x.sin() / x).log10();
        assert!((db + 3.9224).abs() < 0.001, "{db}");
        let at_device_limit = {
            let x = core::f64::consts::PI * 0.3;
            20.0 * (x.sin() / x).log10()
        };
        assert!(
            (at_device_limit + 1.3260).abs() < 0.001,
            "{at_device_limit}"
        );
    }

    /// The polynomial is the paper's, and the two properties the collection's DSP contract requires
    /// of every saturator: bounded **exactly** in `f32`, and monotonic.
    #[test]
    fn the_nonlinearity_is_bounded_and_monotonic() {
        assert_eq!(nonlinearity::curve(2.0), nonlinearity::UPPER);
        assert_eq!(nonlinearity::curve(f32::MAX), nonlinearity::UPPER);
        assert_eq!(nonlinearity::curve(-2.0), nonlinearity::LOWER);
        assert_eq!(nonlinearity::curve(f32::MIN), nonlinearity::LOWER);

        let mut previous = f32::NEG_INFINITY;
        for i in -2000..=2000 {
            let x = i as f32 / 1000.0;
            let y = nonlinearity::curve(x);
            assert!(y.is_finite());
            assert!(y >= previous, "not monotonic at {x}");
            assert!((nonlinearity::LOWER..=nonlinearity::UPPER).contains(&y));
            previous = y;
        }

        // The asymmetry is the point: the second harmonic is what the paper's spectra show, so the
        // curve must not be odd.
        assert!((nonlinearity::curve(0.5) + nonlinearity::curve(-0.5)).abs() > 1e-3);
    }

    /// **Longer lines are worse, in the right order** — the product's whole premise. The distortion
    /// each part produces at the reference level is measured out of the model and checked against
    /// the catalogue's own figure for that part, which is a stronger claim than "it rises": the
    /// MN3011 is a low-noise part that sits *below* the shorter MN3007, and a model that had
    /// smoothed the family into a monotonic ramp would pass a monotonicity test and be wrong.
    #[test]
    fn each_lines_distortion_is_its_own_catalogue_figure() {
        for line in Line::ALL {
            let bbd = Bbd::new(line, 1);
            // Second-harmonic distortion of the drive stage at the reference level, measured from
            // the curve itself rather than predicted from the algebra that chose the drive.
            let a = THD_REFERENCE_LEVEL as f64;
            let n = 4096;
            let out: Vec<f32> = (0..n)
                .map(|i| {
                    let t = core::f64::consts::TAU * (i as f64) / n as f64;
                    bbd.distort((a * t.sin()) as f32)
                })
                .collect();
            let fund = amplitude_at(&out, 1.0, n as f64);
            let second = amplitude_at(&out, 2.0, n as f64);
            let third = amplitude_at(&out, 3.0, n as f64);
            let thd = (second * second + third * third).sqrt() / fund;
            let expected = line.thd_typical() as f64;
            assert!(
                (thd - expected).abs() < 0.1 * expected,
                "{line:?}: measured {:.3} %, catalogue says {:.3} %",
                thd * 100.0,
                expected * 100.0
            );
        }

        // And the S/N figures, which *are* monotonic, are what the noise level follows.
        let mut previous = f32::INFINITY;
        for line in Line::ALL {
            let bbd = Bbd::new(line, 1);
            assert!(bbd.noise_level > previous.min(0.0) && bbd.noise_level < 1e-2);
            assert!(
                bbd.noise_level > previous || previous.is_infinite(),
                "{line:?} is not noisier than the shorter line before it"
            );
            previous = bbd.noise_level;
        }
    }

    /// A mis-trimmed unit is three to five times more distorted at the same level, which is the
    /// catalogue's own curve and the single largest source of unit-to-unit variation in the family.
    /// Both directions, because the curve is a U and not a slope.
    #[test]
    fn the_bias_trimmer_is_a_u_with_its_minimum_at_the_centre() {
        let thd = |bias: f32| {
            let mut bbd = Bbd::new(Line::S4096, 1);
            bbd.set_bias(bias);
            let n = 4096;
            let out: Vec<f32> = (0..n)
                .map(|i| {
                    let t = core::f64::consts::TAU * (i as f64) / n as f64;
                    bbd.distort((THD_REFERENCE_LEVEL as f64 * t.sin()) as f32)
                })
                .collect();
            let fund = amplitude_at(&out, 1.0, n as f64);
            let second = amplitude_at(&out, 2.0, n as f64);
            let third = amplitude_at(&out, 3.0, n as f64);
            (second * second + third * third).sqrt() / fund
        };

        let trimmed = thd(0.0);
        let low = thd(-1.0);
        let high = thd(1.0);
        assert!(low > trimmed * 3.0, "{low} vs {trimmed}");
        assert!(high > trimmed * 3.0, "{high} vs {trimmed}");
        assert!(
            low < trimmed * 5.5 && high < trimmed * 5.5,
            "{low} / {high}"
        );
        // Halfway out is between the two, so it is a curve rather than a switch.
        let half = thd(0.5);
        assert!(half > trimmed && half < high);
    }

    /// Silence in, silence out, exactly — with the chip's noise off, which is the only source that
    /// would otherwise keep a "silent" line awake for ever. This is the denormal-flush test too.
    #[test]
    fn silence_in_gives_exactly_zero_out() {
        let mut bbd = Bbd::new(Line::S4096, 1);
        bbd.noise_level = 0.0;
        only_last_tap(&mut bbd);
        let clock = Clock::for_delay(4096, 0.100);
        let dt = 1.0 / FS;
        for i in 0..(FS as usize) {
            let x = if i < 100 { 0.5 } else { 0.0 };
            bbd.process(x, clock, dt, false);
        }
        for _ in 0..1000 {
            let (mix, tail) = bbd.process(0.0, clock, dt, true);
            assert_eq!(mix, 0.0);
            assert_eq!(tail, 0.0);
        }
    }

    /// `clear()` leaves no tail — a `Line` change, a `reset()` and every transition that brings a
    /// line into use all rely on it.
    #[test]
    fn clear_leaves_no_tail() {
        let mut bbd = Bbd::new(Line::S4096, 1);
        bbd.noise_level = 0.0;
        let clock = Clock::for_delay(4096, 0.050);
        let dt = 1.0 / FS;
        for _ in 0..1000 {
            bbd.process(0.9, clock, dt, false);
        }
        bbd.clear();
        for _ in 0..64 {
            assert_eq!(bbd.process(0.0, clock, dt, false).0, 0.0);
        }
    }

    /// Nothing the parameter space contains may produce a NaN, an infinity or an unbounded output,
    /// including the clock being driven far outside anything musical.
    #[test]
    fn it_cannot_blow_up() {
        for line in Line::ALL {
            for mode in [FilterMode::Fixed, FilterMode::Tracking] {
                for &hz in &[700.0, 5_000.0, 48_000.0, 750_000.0] {
                    let mut bbd = Bbd::new(line, 7);
                    bbd.set_filter_mode(mode);
                    bbd.set_bias(1.0);
                    bbd.set_spread(2.0);
                    open_taps(&mut bbd);
                    let clock = Clock::from_hz(hz);
                    let dt = 1.0 / 44_100.0;
                    let mut rng = Rng::new(99);
                    for _ in 0..20_000 {
                        let (mix, tail) = bbd.process(rng.next_bipolar() * 4.0, clock, dt, true);
                        assert!(
                            mix.is_finite() && tail.is_finite(),
                            "{line:?} {mode:?} {hz}"
                        );
                        assert!(mix.abs() < 32.0 && tail.abs() < 32.0, "{mix} {tail}");
                    }
                }
            }
        }
    }

    /// A non-finite input sample is a zero at the device's own seam. The anti-alias filter's state
    /// is recursive and nothing flushes it, so without that a NaN held there would be written into
    /// the line on every tick for ever.
    #[test]
    fn a_non_finite_input_sample_cannot_poison_the_line() {
        let clock = Clock::for_delay(4096, 0.02);
        let dt = 1.0 / FS;
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut poisoned = Bbd::new(Line::S4096, 3);
            let mut reference = Bbd::new(Line::S4096, 3);
            open_taps(&mut poisoned);
            open_taps(&mut reference);
            for i in 0..(0.1 * FS) as usize {
                let x = 0.5 * (i as f32 * 0.01).sin();
                let at = i == 1000;
                let p = poisoned.process(if at { bad } else { x }, clock, dt, true);
                let r = reference.process(if at { 0.0 } else { x }, clock, dt, true);
                assert!(
                    p.0.to_bits() == r.0.to_bits() && p.1.to_bits() == r.1.to_bits(),
                    "{bad} at sample {i}: {p:?} against {r:?}"
                );
            }
        }
    }

    /// Spread collapses the constellation onto the last tap at zero and stretches it earlier above
    /// one, and never walks off either end of the line.
    #[test]
    fn spread_stretches_about_the_last_tap_and_stays_inside_the_line() {
        for line in Line::ALL {
            let n = line.stages();
            for spread in [0.0, 0.5, 1.0, 1.5, 2.0] {
                let taps = line.tap_stages(spread);
                assert!(taps.iter().all(|&t| t >= 1 && t <= n), "{line:?} {spread}");
                assert_eq!(taps[TAPS - 1], n, "the last tap does not move");
                for w in taps.windows(2) {
                    assert!(w[1] >= w[0], "taps must stay in order");
                }
            }
            // At 1.0 they are Panasonic's own ratios.
            let taps = line.tap_stages(1.0);
            for (i, &s) in MN3011_TAP_STAGES.iter().enumerate() {
                let expected = (s as f64 / MN3011_STAGES as f64 * n as f64).round() as usize;
                assert_eq!(taps[i], expected, "{line:?} tap {}", i + 1);
            }
        }
    }
}
