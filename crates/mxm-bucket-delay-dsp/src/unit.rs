//! One line's worth of the product, and the pair of them.
//!
//! # Where the loop is closed, and why it matters more than anything else here
//!
//! Panasonic's own MN3005 echo circuit settles it (catalogue p. 58): the `Echo Control` pot sits
//! across the **output**, after the reconstruction filter and its output amplifier, and its wiper
//! returns through a buffer and a 220 kΩ resistor to the **input summing node**, ahead of the
//! anti-alias filter. So the loop is the *outer* one:
//!
//! ```text
//!   takeoff:  the wet output — after the tap mix, after reconstruction, and after expansion
//!   return:   the input summing node — before compression, before anti-aliasing
//! ```
//!
//! Every lap therefore re-applies the anti-alias filter, the line's nonlinearity, its noise and its
//! output hold, the reconstruction filter and both halves of the compander. **That is why repeats
//! degrade progressively**, and it is the easiest thing in the product to get wrong: dirt applied
//! once at the output gives a clean delay wearing a dirty coat, which is what a listener identifies
//! as fake.
//!
//! # What each mode does, and what it does not
//!
//! `Routing` decides the modulation phase, the feedback matrix and — in the one-input layout alone
//! — whether the second line runs and how the wet reaches the two outputs. It never changes what
//! happens *inside* a line, which is why ping-pong costs nothing in character: it is a change to
//! the feedback matrix and nothing else.
//!
//! **Nothing is ever summed.** Where two lines share one source they are fed the same signal, which
//! is a duplication of an input rather than a mixdown of two — summing two sources would silence
//! anti-correlated material, and that is a defect rather than a wart.

use crate::bbd::{Bbd, FilterMode, Line, TAPS};
use crate::compander::{Compander, Expander};
use crate::{Clock, finite_or_zero, flush};

/// How long a structural change takes to fade the wet out and back in, in seconds.
///
/// **Chosen**, at the short end of what stays inaudible: long enough that a loud tail does not
/// click when it is cut, short enough that switching feels immediate rather than swept.
pub const FADE_S: f32 = 0.006;

/// Below this the wet counts as quiet, and once the quiet has outlasted everything still held and
/// [`SNAP_HOLD_S`] besides, the loop is cleared to exact zero.
///
/// **Chosen**, and the same figure `mxm-mono-00`'s own delay and springs use: −100 dB, inaudible
/// under anything. Without it a decaying loop approaches zero and never arrives, the host is never
/// told the tail ended, and the collection's *doing nothing uses no CPU* rule becomes a claim
/// nobody honours.
pub const SNAP_LEVEL: f32 = 1e-5;

/// How long the quiet has to last *beyond the longest journey anything still held can take* before
/// the snap.
///
/// The journey is the load-bearing part and it is why this is not `mxm-mono-00`'s bare hold: a
/// delay can be silent at its output while holding audio that has not come back yet, so the quiet
/// has to outlast the longest tap before the line can be called empty. Snapping on the output alone
/// would truncate the first repeat of anything played after a gap. [`Core::process`] says what the
/// journey is.
pub const SNAP_HOLD_S: f32 = 0.05;

/// The longest reverse window, in seconds. Preallocated at activation; nothing in the loop
/// allocates.
pub const MAX_REVERSE_S: f32 = 1.0;

/// How far `Wobble` at full depth pulls the clock, as a fraction.
///
/// **Chosen.** Modulating the clock is how this same chip family makes chorus and vibrato, so the
/// depth that matters is the one where the effect stops being flutter and becomes a modulation
/// effect. Fifteen per cent of the clock is a little over two semitones of pitch pull at the
/// extreme, which reaches seasick without reaching broken.
pub const WOBBLE_DEPTH: f64 = 0.15;

/// **How many taps are really feeding the loop**, as a smooth number rather than a count.
///
/// `(Σg)² / Σg²` — the participation ratio. One fader at any gain gives exactly 1; six equal faders
/// give 6; the reference ladder, whose faders differ by 3.5 dB, gives 5.9; a fader creeping up from
/// zero raises it continuously rather than stepping. A literal count would step, and a step in this
/// number is a step in the loop's gain.
#[inline]
pub fn effective_taps(taps: &[f32; TAPS], return_mode: Return) -> f32 {
    if return_mode == Return::Tail {
        // One tap feeds the loop, whatever the mixer is doing for the output.
        return 1.0;
    }
    let sum: f32 = taps.iter().map(|g| g.abs()).sum();
    let power: f32 = taps.iter().map(|g| g * g).sum();
    if power <= f32::EPSILON {
        1.0
    } else {
        (sum * sum / power).clamp(1.0, TAPS as f32)
    }
}

/// **The gain the loop is run at, so the control means one thing at every tap setting.**
///
/// # Two corrections, and what each was for
///
/// `Feedback` is calibrated against a *measured* sustain threshold, so the line sings at nine tenths
/// of the travel — taken with the last tap alone. Opening the six-tap constellation broke that
/// twice, for two different reasons, and both had to be measured out:
///
/// 1. **The mixer summed its faders**, so six taps was six times the takeoff *and* six times the wet
///    at the output. The singing point moved to 16 % of the travel. Reported as *"I cannot figure
///    out how Feedback works"*. Fixed in the mixer, not here: it normalises by `√Σg²` now
///    ([`crate::bbd::Bbd::tick`]), so the faders set shape and the level stays put.
/// 2. **Decorrelated returns still regenerate faster than one**, at equal level. With the mixer
///    fixed the singing point still slid — 91 % at one tap, 67 % at two, 38 % at six. That is this
///    function, and the divisor is [`effective_taps`], fitted against the measurement:
///
/// | Taps feeding the loop | Threshold, as a fraction of one tap's | `1 / effective_taps` |
/// |---|---|---|
/// | 1 | 1.00 | 1.00 |
/// | 2 | 0.55 | 0.50 |
/// | 6 (equal) | 0.18 | 0.17 |
///
/// `examples/feedback_travel.rs` prints where the knob actually sings and is what fitted this.
/// **Re-fit it, do not reason about it, if the loop's structure changes** — it has already been
/// re-fitted twice, and both times the arithmetic that felt obvious was wrong.
#[inline]
pub fn applied_feedback(target: f32, taps: &[f32; TAPS], return_mode: Return) -> f32 {
    target / effective_taps(taps, return_mode)
}

/// What feeds the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Return {
    /// The bank's sum — dense, and builds into a wash.
    Mix,
    /// The last tap alone — a clean long repeat under a busy output. This is the one thing a second
    /// fader bank would buy, and it costs a reconstruction filter and an envelope instead.
    Tail,
}

/// How the two lines are wired to each other and to the outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routing {
    /// Clock modulation in phase, each line returning to its own input. With one source only one
    /// line runs and its wet goes to both outputs.
    Mono,
    /// Modulation in **antiphase** between the lines, each still returning to its own input — the
    /// honest stereo the JUNO chorus makes with two BBDs.
    Stereo,
    /// Antiphase, and each takeoff returns to **the other** line.
    PingPong,
}

/// The structural settings — the ones a change to which has to fade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub line: Line,
    pub routing: Routing,
    pub reverse: bool,
    pub return_mode: Return,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            line: Line::S4096,
            routing: Routing::Stereo,
            reverse: false,
            return_mode: Return::Mix,
        }
    }
}

/// The continuous settings, which move under the audio without a fade because they are smooth.
#[derive(Debug, Clone, Copy)]
pub struct Controls {
    /// Delay time in seconds — but only ever as a way of asking for a clock.
    pub time_s: f64,
    /// Loop gain, before the control curve the plugin applies.
    pub feedback: f32,
    /// **The mix**: nought is the dry alone, one is the wet alone.
    ///
    /// A crossfade rather than a wet level, so the control reaches *pure echo* at one end and
    /// *pure instrument* at the other — the owner's ruling, 2026-09-06, and it is what
    /// `mxm-chorus-06` has meant by Mix since the collection's first effect. **Nought is still
    /// Off**, and still the dry to the bit: `(1 − 0)·dry + 0·wet` is the dry, exactly.
    pub mix: f32,
    pub taps: [f32; TAPS],
    pub spread: f32,
    pub bias: f32,
    pub wobble: f32,
    pub wobble_rate_hz: f32,
    pub filter: FilterMode,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            time_s: 0.25,
            feedback: 0.0,
            mix: 0.5,
            taps: [0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            spread: 1.0,
            bias: 0.0,
            wobble: 0.0,
            wobble_rate_hz: 0.5,
            filter: FilterMode::Fixed,
        }
    }
}

/// The window that `Reverse` buffers and flips into the line.
///
/// It reverses **the external input only, not the loop's return**: reversing the sum would flip the
/// feedback on every lap, which is not a delay with a reverse in front of it but a different effect
/// that happens to alternate direction.
///
/// **Clearing is constant-time.** Every clear runs on the audio thread, within one sample, and the
/// window is a second of audio per half, so history is invalidated rather than zeroed: each half
/// counts how far its latest recording reached, and a cell beyond that reads as zero.
///
/// **A half's count starts again with every recording, not only at a clear.** A half is rewritten
/// only as far as the window in force reaches, so once `Time` has been long and then short, the
/// cells past the short window still hold the long window's audio. A window that grows again would
/// read them — audio from as many windows back as the short ones lasted, which is minutes if Time
/// sat there for minutes. Counting per recording makes a growth a clear for the span it exposes, and
/// changes nothing wherever the window read only what the window before it wrote.
#[derive(Debug, Clone)]
struct Reverse {
    buf: Vec<f32>,
    half: usize,
    window: usize,
    pos: usize,
    recording: usize,
    /// How many cells of each half its latest recording wrote: the one in progress for the half
    /// recording, the one just finished for the half playing. A half is written from its first cell
    /// up, so the cells below this count are exactly the ones that recording wrote.
    written: [usize; 2],
    /// The window last asked for, which the next boundary takes.
    requested: usize,
}

impl Reverse {
    fn new(sample_rate: f32) -> Self {
        let half = ((MAX_REVERSE_S * sample_rate) as usize).max(1);
        Self {
            buf: vec![0.0; half * 2],
            half,
            window: half,
            pos: 0,
            recording: 0,
            written: [0; 2],
            requested: half,
        }
    }

    fn set_window_seconds(&mut self, seconds: f32, sample_rate: f32) {
        let want = ((seconds * sample_rate) as usize).clamp(64, self.half);
        self.requested = want;
        // Only at a window boundary: changing it mid-window would jump the read pointer into the
        // middle of audio it is not playing back yet.
        if self.pos == 0 {
            self.window = want;
        }
    }

    /// The longest a sample recorded now can wait before it is played: the rest of this window and
    /// the whole of the next, which is at most twice the larger of the two.
    #[inline]
    fn hold_samples(&self) -> usize {
        2 * self.window.max(self.requested)
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let write = self.recording * self.half + self.pos;
        self.buf[write] = x;
        // Set, never raised: a recording's first write brings the count back to one, so nothing
        // the half held from an earlier, longer recording counts as written.
        self.written[self.recording] = self.pos + 1;

        let playing = 1 - self.recording;
        let back = self.window - 1 - self.pos;
        let y = if back < self.written[playing] {
            self.buf[playing * self.half + back]
        } else {
            0.0
        };
        self.pos += 1;
        if self.pos >= self.window {
            self.pos = 0;
            self.recording = 1 - self.recording;
        }
        y
    }

    /// Discard the window. Constant-time: see the type's own documentation.
    fn clear(&mut self) {
        self.written = [0; 2];
        self.pos = 0;
        self.recording = 0;
    }
}

/// One bucket brigade, its compander, its reverse window and its clock modulation.
///
/// It does not close its own loop: the feedback matrix belongs to [`Core`], because in ping-pong a
/// line is fed from the *other* one.
#[derive(Debug, Clone)]
pub struct Unit {
    bbd: Bbd,
    compander: Compander,
    tail_expander: Expander,
    reverse: Reverse,
    wobble_phase: f64,
    /// 0.0 or 0.5 — the antiphase the stereo modes run the second line at.
    phase_offset: f64,
    sample_rate: f32,
    running: bool,
    /// The lap at the clock this line last actually ran at, `Wobble` included.
    lap_s: f64,
    /// The longest the reverse window can hold what it recorded last, or zero while it is out.
    reverse_hold_s: f64,
}

/// What one line hands back per sample.
#[derive(Debug, Clone, Copy)]
pub struct UnitOut {
    /// The tap mix, which is what the output carries.
    pub wet: f32,
    /// What the loop is fed: the mix, or the last tap alone under `Return::Tail`.
    pub takeoff: f32,
}

impl Unit {
    pub fn new(line: Line, sample_rate: f32, seed: u32, phase_offset: f64) -> Self {
        Self {
            bbd: Bbd::new(line, seed),
            compander: Compander::new(sample_rate),
            tail_expander: Expander::new(sample_rate),
            reverse: Reverse::new(sample_rate),
            wobble_phase: 0.0,
            phase_offset,
            sample_rate,
            running: true,
            lap_s: 0.0,
            reverse_hold_s: 0.0,
        }
    }

    /// The clock this line is running at: the delay the player asked for, pulled by `Wobble`.
    ///
    /// The modulation is on the **clock**, which is the only place a bucket brigade has to put it,
    /// and it is why the wobble bends pitch rather than merely moving a read pointer.
    #[inline]
    fn clock(&self, c: &Controls) -> Clock {
        let base = Clock::for_delay(self.bbd.line().stages(), c.time_s).hz();
        let phase = self.wobble_phase + self.phase_offset;
        let lfo = (core::f64::consts::TAU * phase).sin();
        Clock::from_hz(base * (1.0 + WOBBLE_DEPTH * c.wobble as f64 * lfo))
    }

    /// One sample. `external` is the input to this line, `feedback_in` what the matrix returns to
    /// it — already scaled by the feedback gain, and already chosen from whichever line's takeoff
    /// the routing says.
    #[inline]
    pub fn process(
        &mut self,
        external: f32,
        feedback_in: f32,
        c: &Controls,
        shape: &Shape,
    ) -> UnitOut {
        let external = finite_or_zero(external);
        let feedback_in = finite_or_zero(feedback_in);
        let clock = self.clock(c);
        self.lap_s = clock.delay_seconds(self.bbd.line().stages());
        let dt = 1.0 / self.sample_rate as f64;
        self.wobble_phase += (c.wobble_rate_hz as f64) * dt;
        if self.wobble_phase >= 1.0 {
            self.wobble_phase -= 1.0;
        }

        // Reverse buffers the external input and flips it, ahead of the summing node, so the loop's
        // own return is never reversed.
        let external = if shape.reverse {
            self.reverse
                .set_window_seconds(c.time_s as f32, self.sample_rate);
            self.reverse_hold_s = self.reverse.hold_samples() as f64 * dt;
            self.reverse.process(external)
        } else {
            self.reverse_hold_s = 0.0;
            external
        };

        let sum = external + feedback_in;
        let compressed = self.compander.compress(sum);

        let need_tail = shape.return_mode == Return::Tail;
        let (mix, tail) = self.bbd.process(compressed, clock, dt, need_tail);

        let wet = self.compander.expand(mix);
        let takeoff = if need_tail {
            self.tail_expander.process(tail)
        } else {
            wet
        };

        UnitOut {
            wet: flush(wet),
            takeoff: flush(takeoff),
        }
    }

    /// Apply the settings that do not need a fade.
    pub fn apply(&mut self, c: &Controls) {
        self.bbd.set_tap_gains(c.taps);
        self.bbd.set_spread(c.spread);
        self.bbd.set_bias(c.bias);
        self.bbd.set_filter_mode(c.filter);
    }

    /// Fit a different chip and empty everything. Called only while the wet is faded out.
    pub fn set_line(&mut self, line: Line) {
        self.bbd.set_line(line);
        self.clear();
    }

    /// Empty the line, both filters, both companders and the reverse window.
    ///
    /// A line coming into use starts empty, the reverse window is discarded rather than replayed
    /// into a loop that no longer expects it, and a branch coming into use starts with cleared
    /// filter and envelope state. **No transition preserves an outgoing tail.**
    pub fn clear(&mut self) {
        self.bbd.clear();
        self.compander.reset();
        self.tail_expander.reset();
        self.reverse.clear();
        self.wobble_phase = 0.0;
        self.lap_s = 0.0;
        self.reverse_hold_s = 0.0;
    }

    pub fn line(&self) -> Line {
        self.bbd.line()
    }

    pub fn set_running(&mut self, running: bool) {
        if running != self.running {
            self.running = running;
            if !running {
                self.clear();
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }
}

/// Where a transition has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fade {
    /// Nothing pending; the wet is at full.
    Open,
    /// Taking the wet down before a change.
    Out,
    /// Bringing it back after one.
    In,
    /// Mix is zero: the wet is down, the lines are empty, and the core is not run.
    Parked,
    /// The tail ended: nothing has gone in and nothing has come out for longer than anything held
    /// could take to come out, so the lines are empty and the core is **not run**.
    ///
    /// Distinct from `Parked`, which is the player's decision rather than the signal's, and it is
    /// not merely a cleared line: **the chip's own noise never stops**, so a core that cleared its
    /// line and kept running would refill it with a noise floor within one lap and never reach
    /// exact zero. Stopping is what makes the silence exact, and it is the same fact as the
    /// collection's rule that an effect doing nothing uses no CPU.
    Idle,
}

/// Both lines, the feedback matrix, the dry path and the transitions.
#[derive(Debug, Clone)]
pub struct Core {
    left: Unit,
    right: Unit,
    /// Both lines are **always allocated**, at activation, on the main thread; how many *run* is
    /// decided by the layout and `Routing` together, per sample, and costs nothing to change.
    shape: Shape,
    target: Shape,
    controls: Controls,
    takeoff_left: f32,
    takeoff_right: f32,
    fade: f32,
    state: Fade,
    fade_step: f32,
    /// How long the input and the wet have both been below [`SNAP_LEVEL`]. `f64`, because an `f32`
    /// sum of `1 / fs` rounds on every step: simulated, it crosses 4.05 s 20 ms early at 192 kHz
    /// and 6 s 34 ms early — most of [`SNAP_HOLD_S`] — once a long lap and a reverse hold add up.
    quiet_s: f64,
    /// The longest lap either running line has actually run at over that quiet, `Wobble` included.
    quiet_lap_s: f64,
    /// The longest either reverse window could hold a sample over that quiet; zero while it is out.
    quiet_reverse_s: f64,
    sample_rate: f32,
}

impl Core {
    /// Allocates both lines and both reverse windows. Main thread, at activation.
    pub fn new(sample_rate: f32) -> Self {
        let shape = Shape::default();
        Self {
            left: Unit::new(shape.line, sample_rate, 0x51F7_2A11, 0.0),
            right: Unit::new(shape.line, sample_rate, 0x2C93_66D5, 0.5),
            shape,
            target: shape,
            controls: Controls::default(),
            takeoff_left: 0.0,
            takeoff_right: 0.0,
            fade: 1.0,
            state: Fade::Open,
            fade_step: 1.0 / (FADE_S * sample_rate).max(1.0),
            quiet_s: 0.0,
            quiet_lap_s: 0.0,
            quiet_reverse_s: 0.0,
            sample_rate,
        }
    }

    /// Start the quiet over: nothing is held that the snap has to wait for.
    fn forget_quiet(&mut self) {
        self.quiet_s = 0.0;
        self.quiet_lap_s = 0.0;
        self.quiet_reverse_s = 0.0;
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        *self = Self::new(sample_rate);
    }

    /// The settings that move smoothly. Applied straight away.
    pub fn set_controls(&mut self, c: Controls) {
        self.controls = c;
        self.left.apply(&c);
        self.right.apply(&c);
        if c.mix > 0.0 && matches!(self.state, Fade::Parked) {
            // Off empties rather than freezes, so coming back starts from silence — there is
            // nothing stale to spill.
            self.state = Fade::In;
        }
    }

    /// The settings a change to which fades. Idempotent: asking for what is already in force does
    /// nothing at all, which is what lets the plugin call it every block.
    pub fn set_shape(&mut self, shape: Shape) {
        if shape != self.target {
            self.target = shape;
            if self.state != Fade::Parked {
                self.state = Fade::Out;
            } else {
                // Parked: nothing is audible, so the change is free and needs no fade.
                self.apply_shape();
            }
        }
    }

    fn apply_shape(&mut self) {
        if self.shape.line != self.target.line {
            self.left.set_line(self.target.line);
            self.right.set_line(self.target.line);
        } else {
            // Routing, Reverse and Return all fade for the same reason: a routing change can bring
            // a line into use, take one out or re-point the matrix; a reverse change makes the
            // buffered window meaningless; and a return change swaps which branch feeds the loop,
            // its reconstruction filter and its expander state.
            self.left.clear();
            self.right.clear();
        }
        self.takeoff_left = 0.0;
        self.takeoff_right = 0.0;
        self.shape = self.target;
    }

    /// How many lines run, which the layout and `Routing` decide **together**.
    ///
    /// The only cell with one running line is one-input Mono, which is why a `Routing` change can
    /// start a line — going Mono to Stereo there brings the second one in, from empty, behind the
    /// fade.
    #[inline]
    fn second_line_runs(&self, one_input: bool) -> bool {
        !(one_input && self.shape.routing == Routing::Mono)
    }

    /// One sample, both channels.
    ///
    /// `one_input` is the 1-in → 2-out layout: one source, driving both lines, its dry copied to
    /// both outputs. Otherwise each channel keeps its own source and its own dry, and they are
    /// never summed.
    #[inline]
    pub fn process(&mut self, in_l: f32, in_r: f32, one_input: bool) -> (f32, f32) {
        // Before anything reads them — the dry, the wake and both lines. A non-finite sample is a
        // zero: it carries nothing, and one that reached the loop would stay in it.
        let in_l = finite_or_zero(in_l);
        let in_r = finite_or_zero(in_r);

        // Off is off: the dry passes through untouched and the core is not run.
        if self.state == Fade::Parked {
            return (in_l, if one_input { in_l } else { in_r });
        }

        // An ended tail is the same deal until something arrives. Any non-zero sample wakes it, and
        // so does the loop gain crossing into the region where the line sings on its own noise —
        // which is the wake condition the plan states, and it is a gain crossing rather than one
        // named control.
        if self.state == Fade::Idle {
            let arrived = in_l != 0.0 || (!one_input && in_r != 0.0);
            if !arrived && self.loop_gain() < self.shape.line.starts_at() {
                return (in_l, if one_input { in_l } else { in_r });
            }
            self.state = Fade::Open;
            self.fade = 1.0;
            self.forget_quiet();
        }

        match self.state {
            Fade::Out => {
                self.fade -= self.fade_step;
                if self.fade <= 0.0 {
                    self.fade = 0.0;
                    if self.controls.mix <= 0.0 {
                        self.state = Fade::Parked;
                        self.left.clear();
                        self.right.clear();
                        self.takeoff_left = 0.0;
                        self.takeoff_right = 0.0;
                        return (in_l, if one_input { in_l } else { in_r });
                    }
                    self.apply_shape();
                    self.state = Fade::In;
                }
            }
            Fade::In => {
                self.fade += self.fade_step;
                if self.fade >= 1.0 {
                    self.fade = 1.0;
                    self.state = Fade::Open;
                }
            }
            Fade::Open => {
                // Mix reaching zero fades rather than cuts: a loop carrying a loud tail muted on
                // a sample boundary is a click, and a click is a defect and not a wart.
                if self.controls.mix <= 0.0 {
                    self.state = Fade::Out;
                }
            }
            Fade::Parked | Fade::Idle => unreachable!("handled above"),
        }

        let second = self.second_line_runs(one_input);
        self.right.set_running(second);

        let src_l = in_l;
        let src_r = if one_input { in_l } else { in_r };

        // The matrix. Ping-pong is a change to *this* and to nothing else.
        // **The control asks for a loop gain; this is what delivers it.** See `applied_feedback`:
        // the mixer holds the level, and this holds the *threshold*, so the knob sings in the same
        // place however many taps are open.
        let g = applied_feedback(
            self.controls.feedback,
            &self.controls.taps,
            self.shape.return_mode,
        );
        let (fb_l, fb_r) = match self.shape.routing {
            Routing::PingPong => (g * self.takeoff_right, g * self.takeoff_left),
            _ => (g * self.takeoff_left, g * self.takeoff_right),
        };

        let out_l = self.left.process(src_l, fb_l, &self.controls, &self.shape);
        let out_r = if second {
            self.right.process(src_r, fb_r, &self.controls, &self.shape)
        } else {
            UnitOut {
                wet: out_l.wet,
                takeoff: out_l.takeoff,
            }
        };

        self.takeoff_left = out_l.takeoff;
        self.takeoff_right = out_r.takeoff;

        // Snap to exact zero once nothing has been going in *and* nothing has been coming out for
        // longer than anything still held could take to come out. Both halves are needed: quiet at
        // the output alone means only that whatever is in the line has not come back yet.
        let loudest = src_l
            .abs()
            .max(src_r.abs())
            .max(out_l.wet.abs())
            .max(out_r.wet.abs());
        // **Judged against what the lines have actually done over the quiet, not against the
        // controls now.** The line is clocked in buckets, so a sample's journey is however long its
        // clock took: turning Time down leaves a slow repeat in the buckets that a lap at the new
        // Time says should be out, and Wobble can hold a clock on its slow side for a whole lap. So
        // the longest lap either line ran at since the last loud sample is what the quiet has to
        // outlast — and, in front of it, the longest the reverse window could hold a sample.
        let (lap, reverse) = if second {
            (
                self.left.lap_s.max(self.right.lap_s),
                self.left.reverse_hold_s.max(self.right.reverse_hold_s),
            )
        } else {
            (self.left.lap_s, self.left.reverse_hold_s)
        };
        // **The snap yields to self-oscillation.** A loop above its own sustain threshold is a
        // generator, and the hardware's generator starts from the chip's own noise floor — so
        // clearing the line here would make self-oscillation from silence impossible, which is the
        // behaviour the last tenth of the control exists for. Above the threshold the answer is
        // that the core keeps running; below it, a quiet line is emptied.
        let singing = self.loop_gain() >= self.shape.line.sings_at();
        if loudest > SNAP_LEVEL || singing {
            self.quiet_s = 0.0;
            self.quiet_lap_s = lap;
            self.quiet_reverse_s = reverse;
        } else {
            self.quiet_lap_s = self.quiet_lap_s.max(lap);
            self.quiet_reverse_s = self.quiet_reverse_s.max(reverse);
            self.quiet_s += 1.0 / self.sample_rate as f64;
            if self.quiet_s > self.quiet_reverse_s + self.quiet_lap_s + SNAP_HOLD_S as f64 {
                self.left.clear();
                self.right.clear();
                self.takeoff_left = 0.0;
                self.takeoff_right = 0.0;
                self.forget_quiet();
                self.state = Fade::Idle;
                return (src_l, src_r);
            }
        }

        // **A crossfade, not an added level.** The dry comes down as the wet goes up, so the
        // control reaches the echo alone at the top — which a wet *level* cannot do, however far it
        // is turned. `fade` is the transition's own, and only the wet takes it: a `Line` change must
        // not duck the instrument.
        let mix = self.controls.mix;
        let wet_gain = mix * self.fade;
        let dry_gain = 1.0 - mix;
        (
            flush(dry_gain * src_l + wet_gain * out_l.wet),
            flush(dry_gain * src_r + wet_gain * out_r.wet),
        )
    }

    /// Clear everything the loop holds, immediately: line, filters, compander envelopes, the
    /// reverse buffer and the modulation phase.
    ///
    /// **The dry is not state.** A host's reset clears the wet and the core; with input present the
    /// dry still passes through untouched, which is why the reset proof is stated against silence.
    pub fn reset(&mut self) {
        self.left.clear();
        self.right.clear();
        self.takeoff_left = 0.0;
        self.takeoff_right = 0.0;
        self.forget_quiet();
        self.shape = self.target;
        self.state = if self.controls.mix <= 0.0 {
            Fade::Parked
        } else {
            Fade::Open
        };
        self.fade = 1.0;
    }

    /// The gain one lap of the loop applies, which is what decides both whether the line sings and
    /// how long a tail takes to die.
    ///
    /// Every control that is a term in it changes it: `Feedback`, the tap faders — a fader *is* a
    /// term in the takeoff sum — and `Return`, which swaps which sum is taken. The plugin recomputes
    /// its tail estimate from this rather than latching one, and wakes a parked instance when it
    /// crosses the threshold, whichever control moved it.
    /// The gain one lap of the loop applies, in the units [`Line::sings_at`] is measured in.
    ///
    /// **This is `Feedback` alone**, and that is the point: the mixer averages its faders, so the
    /// takeoff does not scale with how many are open and the control's number is the loop's gain.
    /// The faders used to move it, and the knob was unlearnable for it.
    pub fn loop_gain(&self) -> f32 {
        self.controls.feedback.abs()
    }

    /// Whether the loop is currently holding anything at all. A parked or snapped core holds
    /// nothing, and the plugin uses this to tell a tail from an inert instance.
    pub fn is_quiet(&self) -> bool {
        matches!(self.state, Fade::Parked | Fade::Idle)
    }

    /// How long one lap takes: the delay in force, which the clock decides.
    pub fn lap_seconds(&self) -> f64 {
        Clock::for_delay(self.shape.line.stages(), self.controls.time_s)
            .delay_seconds(self.shape.line.stages())
    }

    /// How long the reverse window can still hold a sample back from the line: the rest of the
    /// window in force and the whole of the one asked for next. Zero while `Reverse` is out.
    ///
    /// **A tail counted from now has to add this in front of its laps**, because a click waiting in
    /// the window has not reached the line yet. The laps themselves need no such help: what is left
    /// of a sample's journey through the buckets is never longer than the lap in force.
    pub fn reverse_hold_seconds(&self) -> f64 {
        self.left.reverse_hold_s.max(self.right.reverse_hold_s)
    }

    pub fn shape(&self) -> Shape {
        self.shape
    }

    /// Whether the core is stopped: either the player turned it off, or its tail ended. Either way
    /// nothing is run and the output is the dry input to the bit.
    pub fn is_parked(&self) -> bool {
        matches!(self.state, Fade::Parked | Fade::Idle)
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rng;

    const FS: f32 = 48_000.0;

    fn core() -> Core {
        let mut c = Core::new(FS);
        c.set_controls(engaged(0.0));
        c
    }

    fn run(c: &mut Core, input: &[f32]) -> Vec<f32> {
        input.iter().map(|&x| c.process(x, x, true).0).collect()
    }

    fn tone(n: usize, hz: f32, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (core::f32::consts::TAU * hz * i as f32 / FS).sin())
            .collect()
    }

    fn silence(n: usize) -> Vec<f32> {
        vec![0.0; n]
    }

    fn rms(y: &[f32]) -> f32 {
        (y.iter().map(|s| s * s).sum::<f32>() / y.len() as f32).sqrt()
    }

    /// Engaged, at a given loop gain, with everything else at the init patch.
    fn engaged(feedback: f32) -> Controls {
        Controls {
            mix: 1.0,
            feedback,
            ..Controls::default()
        }
    }

    /// The init shape with one chip fitted.
    fn with_line(line: Line) -> Shape {
        Shape {
            line,
            ..Shape::default()
        }
    }

    /// What a structural change does to the shape, named for the control that made it.
    type Change = (&'static str, fn(&mut Shape));

    /// The largest step between neighbouring samples — a discontinuity is what a click *is*, and
    /// this is how every transition claim in the plan is judged rather than by ear.
    ///
    /// `mxm-measure`'s observation, which also serves `mxm-mono-01-dsp`'s click hunt; that crate
    /// called the same computation `worst_jump`. A buffer too short to have a step has none, and
    /// zero is the right answer for a transition claim about it.
    fn worst_step(y: &[f32]) -> f64 {
        mxm_measure::observe::worst_step(y).map_or(0.0, |(_, step)| step)
    }

    /// **Off is off.** At Mix zero the output is the dry input *to the bit*, and the core is not
    /// run at all.
    #[test]
    fn at_mix_zero_the_output_is_the_dry_input_to_the_bit() {
        let mut c = core();
        let signal = tone(4800, 220.0, 0.7);
        // Fill the line first, so there is a live tail to be silenced.
        run(&mut c, &signal);

        c.set_controls(Controls {
            mix: 0.0,
            ..Controls::default()
        });
        // The fade has to run out first: a loud tail cut on a sample boundary is a click.
        let fading = run(&mut c, &silence(2 * (FADE_S * FS) as usize));
        assert!(worst_step(&fading) < 0.05, "the fade to Off clicked");
        assert!(c.is_parked(), "the core did not park");

        let probe = tone(4800, 330.0, 0.5);
        let out = run(&mut c, &probe);
        assert_eq!(out, probe, "the dry did not pass through untouched");
    }

    /// Coming back from Off starts from silence rather than spilling a stale repeat — a parked line
    /// that kept its contents would replay audio that may be minutes old.
    #[test]
    fn off_empties_rather_than_freezes() {
        let mut c = core();
        run(&mut c, &tone(9600, 220.0, 0.9));

        c.set_controls(Controls {
            mix: 0.0,
            ..Controls::default()
        });
        run(&mut c, &silence(4800));
        assert!(c.is_parked());

        c.set_controls(engaged(0.0));
        // The delay is 250 ms by default, so nothing may arrive for at least that long.
        let out = run(&mut c, &silence((0.2 * FS) as usize));
        let loudest = out.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!(loudest < 1e-3, "a stale repeat spilled out at {loudest}");
    }

    /// **Transitions are silent.** Each structural control is switched under a live tail and the
    /// output checked for a discontinuity rather than judged by ear.
    #[test]
    fn every_structural_change_is_silent_under_a_live_tail() {
        let changes: [Change; 5] = [
            ("Line", |s| s.line = Line::S1024),
            ("Routing", |s| s.routing = Routing::PingPong),
            ("Routing to Mono", |s| s.routing = Routing::Mono),
            ("Reverse", |s| s.reverse = true),
            ("Return", |s| s.return_mode = Return::Tail),
        ];

        for (name, change) in changes {
            let mut c = core();
            c.set_controls(Controls {
                taps: [1.0; TAPS],
                ..engaged(0.5)
            });

            // A live tail: fill the line, then stop playing.
            run(&mut c, &tone((0.5 * FS) as usize, 220.0, 0.8));
            let before = run(&mut c, &silence(256));
            let quiet_step = worst_step(&before);

            let mut shape = c.shape();
            change(&mut shape);
            c.set_shape(shape);

            let during = run(&mut c, &silence((4.0 * FADE_S * FS) as usize));
            let step = worst_step(&during);
            assert!(
                step < quiet_step.max(0.02) * 2.0 + 0.01,
                "{name} stepped by {step} against {quiet_step} while running"
            );
            assert!(during.iter().all(|s| s.is_finite()));
        }
    }

    /// **`reset()` is immediate**, and its proof is stated against silence: reset clears the wet and
    /// the core, and the dry is not state, so with input present the dry must still pass through a
    /// reset untouched. Both halves.
    #[test]
    fn reset_clears_the_tail_but_never_the_dry() {
        let mut c = core();
        c.set_controls(engaged(0.6));
        run(&mut c, &tone((0.5 * FS) as usize, 220.0, 0.9));

        c.reset();
        let after = run(&mut c, &silence(512));
        assert!(
            after.iter().all(|&s| s == 0.0),
            "a tail survived the reset: {}",
            after.iter().fold(0.0f32, |m, &x| m.max(x.abs()))
        );

        // The dry at half mix, which is where it can be seen at all: `engaged` is the wet alone, and
        // a crossfade at one leaves no dry to make a claim about.
        c.set_controls(Controls {
            mix: 0.5,
            feedback: 0.6,
            ..Controls::default()
        });
        let probe = tone(512, 440.0, 0.5);
        c.reset();
        let out = run(&mut c, &probe);
        for (i, (&o, &p)) in out.iter().zip(probe.iter()).enumerate() {
            // Half the input, exactly: nothing has come back yet at 250 ms, so the whole output is
            // the dry side of the crossfade.
            assert!(
                (o - 0.5 * p).abs() < 1e-6,
                "the dry was disturbed by a reset at sample {i}: {o} against {}",
                0.5 * p
            );
        }
    }

    /// A finite tail has to reach **exact** zero, not merely get quiet: the host is told the tail
    /// has ended, and an asymptotic decay keeps a graph awake for ever.
    #[test]
    fn a_finite_tail_reaches_exact_zero() {
        let mut c = core();
        c.set_controls(engaged(0.4));
        run(&mut c, &tone((0.2 * FS) as usize, 220.0, 0.9));

        let out = run(&mut c, &silence((20.0 * FS) as usize));
        let last = &out[out.len() - 4800..];
        assert!(
            last.iter().all(|&s| s == 0.0),
            "the tail never reached zero; it is still at {}",
            last.iter().fold(0.0f32, |m, &x| m.max(x.abs()))
        );
    }

    /// A short ramp, loud enough to come back well above the snap level.
    fn click() -> Vec<f32> {
        (0..64).map(|i| 0.9 * (1.0 - i as f32 / 64.0)).collect()
    }

    fn peak(y: &[f32]) -> f32 {
        y.iter().fold(0.0f32, |m, &x| m.max(x.abs()))
    }

    /// A repeat counts as having come out at a hundred times the snap level. A snapped one is
    /// exactly zero; a click clocked out several times faster than it went in, through the fixed
    /// 2 kHz filter, peaks near 1e-2, which is why this is not higher.
    const HEARD: f32 = 100.0 * SNAP_LEVEL;

    /// **The snap waits for what is already in the line, not for the lap the knob now asks for.**
    /// Time at 2 s, a click, and three quarters of a lap later Time goes to 0.3 s: the click is
    /// still in the buckets and leaves a quarter of the new lap later. Judged against the new lap,
    /// the quiet had already outlasted it, and the line was cleared with the repeat inside.
    #[test]
    fn shortening_time_does_not_snap_away_a_repeat_still_in_the_line() {
        let mut c = core();
        let mut controls = Controls {
            time_s: 2.0,
            ..engaged(0.0)
        };
        c.set_controls(controls);
        run(&mut c, &click());
        let early = run(&mut c, &silence((1.5 * FS) as usize - 64));
        assert!(peak(&early) < 1e-3, "the repeat arrived early");

        controls.time_s = 0.3;
        c.set_controls(controls);
        let after = run(&mut c, &silence((0.5 * FS) as usize));
        assert!(
            peak(&after) > HEARD,
            "the repeat was snapped away: {:e}, parked {}",
            peak(&after),
            c.is_parked()
        );
        run(&mut c, &silence(FS as usize));
        assert!(
            c.is_parked(),
            "and the line still has to snap once it is empty"
        );
    }

    /// **The reverse window is part of the journey.** It holds a sample for up to two windows before
    /// the line sees it, and the window follows Time — so at 250 ms a click played into a quiet
    /// instance reaches the line half a second later and the output a lap after that, while a snap
    /// judged against one lap cleared the window, click and all, at 300 ms.
    #[test]
    fn the_snap_waits_for_a_click_held_in_the_reverse_window() {
        let mut c = core();
        c.set_controls(Controls {
            time_s: 0.25,
            ..engaged(0.0)
        });
        c.set_shape(Shape {
            reverse: true,
            ..Shape::default()
        });
        run(&mut c, &silence(FS as usize));
        assert!(c.is_parked(), "the instance did not reach its quiet start");

        let mut input = click();
        input.extend(silence((1.5 * FS) as usize));
        let out = run(&mut c, &input);
        assert!(
            peak(&out) > HEARD,
            "the reversed click never came out: {:e}",
            peak(&out)
        );
        run(&mut c, &silence(FS as usize));
        assert!(
            c.is_parked(),
            "and the line still has to snap once it is empty"
        );
    }

    /// **The lap is the one the line actually runs at.** Wobble pulls the clock by up to fifteen
    /// per cent, and a slow rate holds one line on the slow side for a whole lap: at 2 s its click
    /// comes back about a tenth of a second late, which a snap judged against the nominal lap cut.
    /// The click goes to the second line alone, whose antiphase starts it slowing.
    #[test]
    fn wobble_slowing_the_clock_does_not_snap_away_a_repeat() {
        let mut c = core();
        c.set_controls(Controls {
            time_s: 2.0,
            wobble: 1.0,
            wobble_rate_hz: 0.05,
            ..engaged(0.0)
        });
        let n = (3.0 * FS) as usize;
        let mut input = click();
        input.resize(n, 0.0);
        let right: Vec<f32> = input.iter().map(|&x| c.process(0.0, x, false).1).collect();
        let arrived = right
            .iter()
            .position(|x| x.abs() > 1e-3)
            .map(|i| i as f32 / FS);
        assert!(
            peak(&right) > HEARD,
            "the slowed repeat was snapped away: {:e}, parked {}",
            peak(&right),
            c.is_parked()
        );
        assert!(
            arrived.is_some_and(|t| t > 2.0 + SNAP_HOLD_S),
            "the clock was not slow enough to test the snap: it arrived at {arrived:?}"
        );
        for _ in 0..(3.0 * FS) as usize {
            c.process(0.0, 0.0, false);
        }
        assert!(
            c.is_parked(),
            "and the line still has to snap once it is empty"
        );
    }

    /// Ping-pong is a change to the feedback matrix and nothing else, so it must actually cross the
    /// channels — and with two different sources the two sides must stay different.
    #[test]
    fn ping_pong_returns_each_takeoff_to_the_other_line() {
        let mut c = Core::new(FS);
        c.set_controls(Controls {
            time_s: 0.05,
            ..engaged(0.7)
        });
        let mut shape = c.shape();
        shape.routing = Routing::PingPong;
        c.set_shape(shape);
        // Let the transition finish before playing anything: the fade deliberately empties the
        // lines, so audio played into it would be wiped rather than delayed.
        for _ in 0..(4.0 * FADE_S * FS) as usize {
            c.process(0.0, 0.0, false);
        }

        // A click into the left channel only, then silence, with two separate inputs.
        let mut left_energy = 0.0f32;
        let mut right_energy = 0.0f32;
        for i in 0..(FS as usize) {
            let x = if i < 32 { 0.9 } else { 0.0 };
            let (l, r) = c.process(x, 0.0, false);
            if i > (0.5 * FS) as usize {
                left_energy += l * l;
                right_energy += r * r;
            }
        }
        assert!(
            right_energy > 0.0,
            "nothing crossed to the right channel at all"
        );
        assert!(left_energy > 0.0);
    }

    /// **It cannot blow up.** Bounded and NaN-free across the parameter space, including the
    /// self-oscillating region and silence in.
    #[test]
    fn it_cannot_blow_up() {
        let mut rng = Rng::new(4242);
        for &feedback in &[0.0, 0.5, 0.95, 1.0, 1.5] {
            for line in Line::ALL {
                for routing in [Routing::Mono, Routing::Stereo, Routing::PingPong] {
                    for return_mode in [Return::Mix, Return::Tail] {
                        let mut c = Core::new(FS);
                        c.set_controls(Controls {
                            taps: [1.0; TAPS],
                            bias: 1.0,
                            wobble: 1.0,
                            wobble_rate_hz: 8.0,
                            time_s: 0.03,
                            ..engaged(feedback)
                        });
                        c.set_shape(Shape {
                            line,
                            routing,
                            reverse: true,
                            return_mode,
                        });

                        for i in 0..20_000 {
                            let x = if i < 5000 {
                                rng.next_bipolar() * 2.0
                            } else {
                                0.0
                            };
                            let (l, r) = c.process(x, -x, false);
                            assert!(
                                l.is_finite() && r.is_finite(),
                                "{line:?} {routing:?} {return_mode:?} at feedback {feedback}"
                            );
                            assert!(
                                l.abs() < 64.0 && r.abs() < 64.0,
                                "{line:?} {routing:?} {return_mode:?} at feedback {feedback}: \
                                 {l} / {r} after {i}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// **The loop gain is `Feedback` alone, whatever the faders are doing** — and that is the fix
    /// rather than a simplification.
    ///
    /// It used to be the control times the takeoff sum, which is arithmetically true of a summing
    /// mixer and unusable as a control: opening the six-tap constellation moved the singing point
    /// from nine tenths of the travel to one sixth of it. The mixer averages now, so what the
    /// control says is what the loop does.
    #[test]
    fn the_loop_gain_is_feedback_alone_whatever_the_faders_do() {
        let mut c = core();
        let mut controls = Controls {
            feedback: 0.5,
            taps: [0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            ..Controls::default()
        };
        c.set_controls(controls);
        assert_eq!(c.loop_gain(), 0.5);

        controls.taps = [1.0; TAPS];
        c.set_controls(controls);
        assert_eq!(
            c.loop_gain(),
            0.5,
            "opening five more faders must not move the control's meaning"
        );

        let mut shape = c.shape();
        shape.return_mode = Return::Tail;
        c.set_shape(shape);
        run(&mut c, &silence((3.0 * FADE_S * FS) as usize));
        assert_eq!(c.loop_gain(), 0.5, "nor may switching what feeds the loop");
    }

    /// **The effective tap count is the participation ratio**, and it is what holds the singing
    /// point still. A literal count would step as a fader crossed zero; a step here is a step in
    /// the loop's gain, which is a jump in how hard the delay is regenerating.
    #[test]
    fn the_effective_tap_count_is_smooth_and_matches_what_is_open() {
        let one = [0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let half = [0.0, 0.0, 0.0, 0.0, 0.0, 0.5];
        let two = [0.0, 0.0, 0.0, 1.0, 0.0, 1.0];
        let all = [1.0; TAPS];
        let ladder = [1.0, 0.909, 0.833, 0.769, 0.714, 0.667];

        assert!((effective_taps(&one, Return::Mix) - 1.0).abs() < 1e-5);
        assert!(
            (effective_taps(&half, Return::Mix) - 1.0).abs() < 1e-5,
            "one fader is one tap at any gain: the fader is a weight, not a count"
        );
        assert!((effective_taps(&two, Return::Mix) - 2.0).abs() < 1e-5);
        assert!((effective_taps(&all, Return::Mix) - 6.0).abs() < 1e-5);
        // The reference ladder's faders differ by 3.5 dB, so it is a little under six.
        let ladder_taps = effective_taps(&ladder, Return::Mix);
        assert!(
            (5.8..6.0).contains(&ladder_taps),
            "the ladder counts as {ladder_taps} taps"
        );

        // Smooth: a fader creeping up from nothing raises the count continuously.
        let mut previous = 1.0;
        for step in 0..=20 {
            let g = step as f32 / 20.0;
            let taps = [0.0, 0.0, 0.0, 0.0, g, 1.0];
            let now = effective_taps(&taps, Return::Mix);
            assert!(
                now >= previous - 1e-6 && now - previous < 0.12,
                "{g}: {previous} to {now}"
            );
            previous = now;
        }

        // `Return = Tail` feeds the loop one tap whatever the mixer is doing for the output.
        assert_eq!(effective_taps(&all, Return::Tail), 1.0);
    }

    /// **The singing point stays where the control says it is**, at every tap setting.
    ///
    /// The property the normalisation exists for, asserted where it can be cheap: the gain the loop
    /// is actually run at, times the takeoff it is applied to, is the same number however the
    /// faders are set. `examples/feedback_travel.rs` is the expensive form — it sweeps the knob and
    /// listens — and is what fitted the law.
    #[test]
    fn the_takeoff_is_normalised_out_at_every_tap_setting() {
        let ladder = [1.0, 0.909, 0.833, 0.769, 0.714, 0.667];
        let one = [0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let half = [0.0, 0.0, 0.0, 0.0, 0.0, 0.5];
        let all = [1.0; TAPS];

        // The mixer's output is the *shape* at a level the faders do not set: the same input,
        // measured through each setting, comes out at the same level.
        let level = |taps: [f32; TAPS]| {
            let mut bbd = crate::bbd::Bbd::new(Line::S4096, 1);
            bbd.set_tap_gains(taps);
            bbd.set_filter_mode(crate::bbd::FilterMode::Tracking);
            let clock = crate::Clock::for_delay(4096, 0.05);
            let dt = 1.0 / FS as f64;
            let mut peak = 0.0f32;
            for i in 0..(0.4 * FS) as usize {
                let x = (core::f32::consts::TAU * 220.0 * i as f32 / FS).sin();
                let (mix, _) = bbd.process(x, clock, dt, false);
                if i > (0.2 * FS) as usize {
                    peak = peak.max(mix.abs());
                }
            }
            peak
        };

        let reference = level(one);
        assert!(reference > 0.1, "the reference tap produced nothing");
        for taps in [half, all, ladder] {
            let got = level(taps);
            assert!(
                (got / reference - 1.0).abs() < 0.35,
                "{taps:?} came out at {got} against the single tap's {reference}: the faders are                  setting the level, not the shape"
            );
        }
    }

    /// **A sleeping instance wakes on a parameter alone.** A parked, silent instance with no input,
    /// carried into the self-starting region by each control that can now do it.
    ///
    /// **Two controls, not four.** Before the takeoff was normalised, a tap fader or `Return` could
    /// carry the loop over the threshold on their own; they cannot now, by design, and the wake
    /// condition is still *the loop gain crossed* — there are simply fewer ways to cross it. `Line`
    /// is the second because each chip has its own measured threshold.
    #[test]
    fn a_parked_instance_wakes_when_the_loop_gain_crosses_the_threshold() {
        for line in [Line::S4096, Line::S1024] {
            let full = crate::bbd::feedback_gain(line, 1.0);
            let mut c = Core::new(FS);
            c.set_controls(engaged(0.0));
            c.set_shape(with_line(line));
            run(&mut c, &silence(FS as usize));
            assert!(c.is_parked(), "{line:?} did not park on silence");

            c.set_controls(engaged(full));
            let out = run(&mut c, &silence((12.0 * FS) as usize));
            let level = rms(&out[out.len() - 4800..]);
            assert!(
                level > 1e-3,
                "{line:?}: a silent instance did not wake when Feedback crossed ({level:e})"
            );
        }
    }

    /// **One bad sample cannot poison the loop.** A NaN or an infinity reaching the line, the
    /// compander or the feedback takeoff stays there — every lap re-reads it — and the only thing
    /// that would ever clear it is the idle snap, which needs silence. So a host that sends one,
    /// followed by ordinary audio, must get exactly what it would have got had the sample been
    /// zero: both layouts, with the loop, all six taps and the reverse window in use.
    #[test]
    fn a_non_finite_input_sample_cannot_poison_the_loop() {
        let controls = Controls {
            time_s: 0.05,
            mix: 0.5,
            feedback: 0.5,
            taps: [1.0; TAPS],
            ..Controls::default()
        };
        let shape = Shape {
            reverse: true,
            ..Shape::default()
        };
        let n = FS as usize;
        for one_input in [true, false] {
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut poisoned = Core::new(FS);
                let mut reference = Core::new(FS);
                for c in [&mut poisoned, &mut reference] {
                    c.set_controls(controls);
                    c.set_shape(shape);
                }
                let mut energy = 0.0f64;
                for i in 0..n {
                    let x = 0.6 * (core::f32::consts::TAU * 220.0 * i as f32 / FS).sin();
                    let ((pa, pb), (ra, rb)) = if i == n / 4 {
                        ((bad, -bad), (0.0, 0.0))
                    } else {
                        ((x, -x), (x, -x))
                    };
                    let (pl, pr) = poisoned.process(pa, pb, one_input);
                    let (rl, rr) = reference.process(ra, rb, one_input);
                    assert!(
                        pl.to_bits() == rl.to_bits() && pr.to_bits() == rr.to_bits(),
                        "{bad} with one input {one_input}: sample {i} gave {pl} / {pr} against \
                         {rl} / {rr}"
                    );
                    if i > n / 4 {
                        energy += (rl * rl + rr * rr) as f64;
                    }
                }
                assert!(energy > 1.0, "the render after the bad sample was silent");
            }
        }
    }

    /// The same at the line's own seam, which takes the external input and the returned takeoff
    /// separately: a bad sample on either is a zero, and nothing downstream remembers it.
    #[test]
    fn a_line_is_not_poisoned_by_a_non_finite_input_or_return() {
        let controls = Controls {
            time_s: 0.05,
            taps: [1.0; TAPS],
            ..Controls::default()
        };
        let shape = Shape {
            reverse: true,
            return_mode: Return::Tail,
            ..Shape::default()
        };
        let n = (0.5 * FS) as usize;
        for poison_the_return in [false, true] {
            for bad in [f32::NAN, f32::INFINITY] {
                let mut poisoned = Unit::new(Line::S4096, FS, 7, 0.0);
                let mut reference = Unit::new(Line::S4096, FS, 7, 0.0);
                poisoned.apply(&controls);
                reference.apply(&controls);
                let mut returned = 0.0f32;
                for i in 0..n {
                    let x = 0.6 * (core::f32::consts::TAU * 220.0 * i as f32 / FS).sin();
                    let fb = 0.4 * returned;
                    let at = i == n / 4;
                    let p = if !at {
                        poisoned.process(x, fb, &controls, &shape)
                    } else if poison_the_return {
                        poisoned.process(x, bad, &controls, &shape)
                    } else {
                        poisoned.process(bad, fb, &controls, &shape)
                    };
                    let r = if !at {
                        reference.process(x, fb, &controls, &shape)
                    } else if poison_the_return {
                        reference.process(x, 0.0, &controls, &shape)
                    } else {
                        reference.process(0.0, fb, &controls, &shape)
                    };
                    assert!(
                        p.wet.to_bits() == r.wet.to_bits()
                            && p.takeoff.to_bits() == r.takeoff.to_bits(),
                        "{bad} on the {} at sample {i}: {} / {} against {} / {}",
                        if poison_the_return { "return" } else { "input" },
                        p.wet,
                        p.takeoff,
                        r.wet,
                        r.takeoff
                    );
                    returned = r.takeoff;
                }
            }
        }
    }

    /// **Clearing the reverse window is constant-time.** It runs within one audio sample of a
    /// `Routing`, `Reverse` or `Return` fade reaching zero, of parking and of the idle snap, on
    /// both lines, and the window is a second of audio per half: zeroing it there was four times
    /// the sample rate in writes inside one sample. So history is invalidated, not visited — and
    /// that is only a fix if what was invalidated can never be read back, which is the second half.
    #[test]
    fn clearing_the_reverse_window_invalidates_history_without_visiting_the_allocation() {
        let mut used = Reverse::new(FS);
        for n in 0..4 * used.half {
            used.set_window_seconds(MAX_REVERSE_S, FS);
            used.process(1.0 + n as f32);
        }
        let history = used.buf.clone();
        assert!(history.iter().all(|&s| s != 0.0), "the window was not full");

        used.clear();
        assert!(
            used.buf == history,
            "clear rewrote {} of {} cells",
            used.buf
                .iter()
                .zip(&history)
                .filter(|(a, b)| a != b)
                .count(),
            history.len()
        );

        // What the cleared window replays is exactly what a fresh one does — including after the
        // window grows past what has been written since, which is where a stale cell would be read.
        let mut fresh = Reverse::new(FS);
        let mut i = 0usize;
        for (window_s, samples) in [(0.1, (0.25 * FS) as usize), (1.0, (2.5 * FS) as usize)] {
            for _ in 0..samples {
                let x = (i as f32 * 0.37).sin();
                used.set_window_seconds(window_s, FS);
                fresh.set_window_seconds(window_s, FS);
                let (a, b) = (used.process(x), fresh.process(x));
                assert!(
                    a.to_bits() == b.to_bits(),
                    "a stale sample returned at {i}: {a} against {b}"
                );
                i += 1;
            }
        }
    }

    /// **A window that grows plays only what the window before it recorded.** The two halves take
    /// turns, and a half is rewritten only as far as the window in force reaches, so after a long
    /// window and then short ones, the cells past the short window still hold the long window's
    /// audio. Growing the window again reads them: a clear protects nothing here, because there was
    /// none. The reference is the rule itself — the last window's recording, reversed, and silence
    /// past its end — taken at the boundaries the window actually keeps.
    #[test]
    fn a_growing_window_plays_only_what_the_last_window_recorded() {
        let mut reverse = Reverse::new(FS);
        let mut previous: Vec<f32> = Vec::new();
        let mut current: Vec<f32> = Vec::new();
        let (mut stale, mut first, mut loudest) = (0usize, None, 0.0f32);
        let mut i = 0usize;
        // A long window of loud audio, short windows of quiet audio, then the long window again;
        // each change asked for part way through a window, as Time moves.
        for (window_s, seconds, level) in [(1.0, 2.5, 0.9), (0.1, 1.05, 1e-3), (1.0, 2.5, 1e-3)] {
            for _ in 0..(seconds * FS) as usize {
                let x = level * (i as f32 * 0.37).sin();
                reverse.set_window_seconds(window_s, FS);
                if reverse.pos == 0 {
                    previous = std::mem::take(&mut current);
                }
                let back = reverse.window - 1 - reverse.pos;
                let expected = previous.get(back).copied().unwrap_or(0.0);
                current.push(x);
                let got = reverse.process(x);
                if got.to_bits() != expected.to_bits() {
                    stale += 1;
                    first.get_or_insert(i);
                    loudest = loudest.max(got.abs());
                }
                i += 1;
            }
        }
        assert!(
            stale == 0,
            "{stale} samples replayed audio no recent window recorded, the first at {:.3} s, \
             the loudest at {loudest}",
            first.unwrap_or(0) as f32 / FS
        );
    }

    /// **The same through the line, as a player hears it.** Loud audio at Time 1 s, then quiet
    /// audio at 100 ms long enough for every repeat of the loud part to have left the line, then
    /// Time back to 1 s. The reference is a twin given silence where the other was given the loud
    /// part, so every cell it wrote then is zero: whatever the two still disagree about once Time is
    /// long again is the loud part coming back out of the window.
    #[test]
    fn lengthening_time_does_not_replay_audio_from_an_earlier_longer_window() {
        let shape = Shape {
            reverse: true,
            routing: Routing::Mono,
            ..Shape::default()
        };
        let mut heard = core();
        let mut reference = core();
        for c in [&mut heard, &mut reference] {
            c.set_shape(shape);
            run(c, &silence((3.0 * FADE_S * FS) as usize));
        }
        let mut noise = Rng::new(99);
        let mut i = 0usize;
        // After the loud part both are fed the same quiet tone, which keeps both awake: neither
        // snaps, so both run the same chip noise.
        let mut phase =
            |time_s: f64, seconds: f32, loud: bool, heard: &mut Core, reference: &mut Core| {
                let controls = Controls {
                    time_s,
                    ..engaged(0.0)
                };
                heard.set_controls(controls);
                reference.set_controls(controls);
                let mut difference = Vec::with_capacity((seconds * FS) as usize);
                for _ in 0..(seconds * FS) as usize {
                    let quiet = 1e-3 * (core::f32::consts::TAU * 220.0 * i as f32 / FS).sin();
                    let (x, y) = if loud {
                        (0.5 * noise.next_bipolar(), 0.0)
                    } else {
                        (quiet, quiet)
                    };
                    let a = heard.process(x, x, true).0;
                    let b = reference.process(y, y, true).0;
                    difference.push(a - b);
                    i += 1;
                }
                difference
            };
        phase(1.0, 2.5, true, &mut heard, &mut reference);
        phase(0.1, 3.0, false, &mut heard, &mut reference);
        let after = phase(1.0, 3.0, false, &mut heard, &mut reference);
        assert!(!heard.is_parked() && !reference.is_parked());
        // By then the two have converged to the bit, so any difference at all is the loud part.
        let differing = after.iter().filter(|d| **d != 0.0).count();
        assert!(
            differing == 0,
            "lengthening Time replayed the loud part: {differing} samples differ from a twin that \
             never heard it, {:e} RMS",
            rms(&after)
        );
    }

    /// One input drives both lines in the stereo modes, and that is a duplication of an input
    /// rather than a mixdown: the single running line of one-input Mono is the only cell where a
    /// line is idle.
    #[test]
    fn one_input_mono_runs_one_line_and_the_stereo_modes_run_two() {
        let mut c = core();
        let mut shape = c.shape();
        shape.routing = Routing::Mono;
        c.set_shape(shape);
        run(&mut c, &tone(4800, 220.0, 0.5));
        assert!(!c.right.is_running(), "Mono with one source ran both lines");

        shape.routing = Routing::Stereo;
        c.set_shape(shape);
        run(&mut c, &tone(4800, 220.0, 0.5));
        assert!(c.right.is_running(), "Stereo with one source ran one line");
    }
}
