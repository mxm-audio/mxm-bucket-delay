//! The compander, which is why an analogue delay's repeats *breathe*.
//!
//! Raffel & Smith §3. A bucket brigade's dynamic range is bounded below by its noise and above by
//! its distortion, and both get worse with stage count, so a long line is companded and a short one
//! is not. The chip is almost always a **570/571-series** part: two variable-gain amplifiers and
//! two level averagers, with the compression and expansion ratio **fixed internally at 2**. One
//! half is a feedback compressor before the line, the other a feedforward expander after it.
//!
//! The fixed ratio is worth stating plainly, because it is not a subtlety: the compressor squeezes
//! the signal to half its dynamic range *in dB*, pushes it through the line near full scale, and
//! the expander stretches it back. That is why the noise floor rises and falls with the signal
//! instead of sitting still.
//!
//! # Two departures, both declared
//!
//! - **Panasonic's own reference echo circuits have no compander at all** — they get 60 dB S/N from
//!   filtering alone. Companding is a pedal builder's answer to the same problem, not the
//!   manufacturer's. It is fitted here because this product's longest line is twice the longest
//!   part Panasonic sold, and because it is what the two DAFx papers model.
//! - **Its position is ours.** Raffel & Smith report the two halves as usually sitting *outside*
//!   the filters (§3.2), and outside is also the only position consistent with the loop's takeoff
//!   being the wet output (the plan's §2). Panasonic's circuits, having no compander, settle
//!   nothing about where one goes.

use crate::{finite_or_zero, flush};

/// The averager's time constant, in seconds.
///
/// **Chosen from a measured range.** The 570/571's rectifier feeds an RC low-pass whose resistor is
/// the chip's internal 10 kΩ and whose capacitor is external; in BBD systems that capacitor is
/// *"normally chosen to be between 0.22 µF and 1 µF"*, so `τ = 10000 · C` puts the real range at
/// 2.2 ms to 10 ms. This takes the middle of it. What would replace the choice: the capacitor
/// fitted in a specific box, if this product ever decides it is copying one.
pub const AVERAGER_TAU_S: f32 = 0.005;

/// The floor under the averager, as a linear amplitude.
///
/// The compressor divides by its own averaged output, so without a floor a silent input is a
/// division by zero and the first sample after silence is unbounded. Real 571s have a floor of the
/// same kind — the rectifier's own offset and leakage — and this one is set at −60 dB, which caps
/// the compressor's gain at 60 dB.
pub const AVERAGE_FLOOR: f32 = 1e-3;

/// The compressor's output bound.
///
/// A step from silence outruns the averager for as long as its attack takes, so the compressor
/// overshoots — which is real, and is part of why a compander *breathes*. It is bounded rather than
/// left to be large: at the floor the gain is 60 dB, and an unclamped overshoot would drive the
/// line's polynomial onto its clamp and buzz. Twice full scale is the headroom a BBD's own input
/// stage has before it limits.
pub const COMPRESSOR_BOUND: f32 = 2.0;

/// A full-wave rectifier into a one-pole averager — the level detector both halves share.
///
/// `y[n] = x[n]·T/(RC+T) + y[n−1]·RC/(RC+T)`, which is Raffel & Smith's own form.
#[derive(Debug, Clone)]
pub struct Averager {
    state: f32,
    coeff: f32,
}

impl Averager {
    pub fn new(sample_rate: f32) -> Self {
        let mut a = Self {
            state: 0.0,
            coeff: 0.0,
        };
        a.set_sample_rate(sample_rate);
        a
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        let t = 1.0 / sample_rate.max(1.0);
        self.coeff = t / (AVERAGER_TAU_S + t);
    }

    /// Rectify and average one sample, returning the average with the floor applied.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.state = flush(self.state + self.coeff * (x.abs() - self.state));
        self.state.max(AVERAGE_FLOOR)
    }

    #[inline]
    pub fn level(&self) -> f32 {
        self.state.max(AVERAGE_FLOOR)
    }

    #[inline]
    pub fn reset(&mut self) {
        self.state = 0.0;
    }
}

/// The most the expander will multiply by.
///
/// Its gain is `avg(|x|)`, which for anything at or below full scale is at or below one — so this
/// bites only above full scale, where a real 571 has run out of headroom too. It is what stops a
/// wide-open six-tap mix from being *squared* into the loop: without it the expander is a squarer,
/// and a squarer inside a feedback path is unbounded however small the feedback gain is.
pub const EXPANDER_MAX_GAIN: f32 = 2.0;

/// The feedforward half on its own: `y = avg(|x|)·x`.
///
/// It is separate from [`Compander`] because the loop needs a *second* one. With `Return = Tail`
/// the takeoff carries tap 6 alone while the output carries the six-tap mix, and those are two
/// different signals — so they cannot share an envelope follower, any more than they can share a
/// reconstruction filter. The plan's §2 calls that a declared departure from a circuit that has one
/// output; the cost is an envelope, not a second line.
#[derive(Debug, Clone)]
pub struct Expander {
    average: Averager,
}

impl Expander {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            average: Averager::new(sample_rate),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.average.set_sample_rate(sample_rate);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let x = finite_or_zero(x);
        let gain = self.average.process(x).min(EXPANDER_MAX_GAIN);
        flush(x * gain)
    }

    #[inline]
    pub fn reset(&mut self) {
        self.average.reset();
    }
}

/// The two halves, as one object per line.
///
/// The compressor is a **feedback** design — it averages its own *output* — which is what fixes the
/// ratio at 2 without a logarithm anywhere: in the steady state `y = x/avg(|y|)` and `avg(|y|) = y`
/// give `y = √x`, and a square root is a halving of the dynamic range in dB. The expander is
/// feedforward, `y = avg(|x|)·x`, and squares it back. The two compose to unity, which
/// [`Compander`] is tested for.
#[derive(Debug, Clone)]
pub struct Compander {
    compressor_average: Averager,
    expander: Expander,
}

impl Compander {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            compressor_average: Averager::new(sample_rate),
            expander: Expander::new(sample_rate),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.compressor_average.set_sample_rate(sample_rate);
        self.expander.set_sample_rate(sample_rate);
    }

    /// Into the line: squeeze the dynamic range in half.
    ///
    /// The averager sees the previous output rather than this one, which is what makes a feedback
    /// topology computable without solving an implicit equation each sample — and is what the
    /// hardware does too, since its detector is downstream of its own VCA.
    #[inline]
    pub fn compress(&mut self, x: f32) -> f32 {
        // A detector holding a NaN reports its floor for ever, because `f32::max` ignores one.
        let x = finite_or_zero(x);
        let gain = 1.0 / self.compressor_average.level();
        let y = (x * gain).clamp(-COMPRESSOR_BOUND, COMPRESSOR_BOUND);
        self.compressor_average.process(y);
        flush(y)
    }

    /// Out of the line: stretch it back.
    #[inline]
    pub fn expand(&mut self, x: f32) -> f32 {
        self.expander.process(x)
    }

    /// Clear both detectors. A branch coming into use starts with cleared filter and envelope
    /// state — the plan's §4 says so of every transition — and a `reset()` must leave none of this
    /// behind either.
    #[inline]
    pub fn reset(&mut self) {
        self.compressor_average.reset();
        self.expander.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    fn settle(c: &mut Compander, amplitude: f32, seconds: f32) -> (f32, f32) {
        let n = (FS * seconds) as usize;
        let (mut peak_mid, mut peak_out) = (0.0f32, 0.0f32);
        for i in 0..n {
            let t = core::f32::consts::TAU * 220.0 * (i as f32) / FS;
            let x = amplitude * t.sin();
            let mid = c.compress(x);
            let out = c.expand(mid);
            // Only the last tenth counts: the averagers have to settle first.
            if i > n - n / 10 {
                peak_mid = peak_mid.max(mid.abs());
                peak_out = peak_out.max(out.abs());
            }
        }
        (peak_mid, peak_out)
    }

    /// The pair composes to unity, which is the only reason it is safe to have in the signal path
    /// at all: whatever the compressor does to the dynamics, the expander undoes.
    #[test]
    fn compressing_then_expanding_returns_the_signal() {
        for &a in &[0.05f32, 0.2, 0.5, 1.0] {
            let mut c = Compander::new(FS);
            let (_, out) = settle(&mut c, a, 0.5);
            assert!(
                (out / a - 1.0).abs() < 0.05,
                "at {a}: {out} came back out, a ratio of {}",
                out / a
            );
        }
    }

    /// **The ratio is 2, and that is the whole point.** Twenty dB in has to become ten dB in the
    /// middle: the line sees a signal squeezed into half the dynamic range, near full scale, which
    /// is what keeps it off the noise floor at the bottom and out of the distortion at the top.
    #[test]
    fn the_compression_ratio_is_two() {
        let mut quiet = Compander::new(FS);
        let mut loud = Compander::new(FS);
        let (mid_quiet, _) = settle(&mut quiet, 0.05, 0.5);
        let (mid_loud, _) = settle(&mut loud, 0.5, 0.5);

        let in_db = 20.0 * (0.5f32 / 0.05).log10();
        let mid_db = 20.0 * (mid_loud / mid_quiet).log10();
        assert!(
            (mid_db - in_db / 2.0).abs() < 1.0,
            "{in_db} dB in became {mid_db} dB in the line; the ratio is {}",
            in_db / mid_db
        );
    }

    /// A quiet signal is lifted toward full scale, which is the noise-reduction argument for having
    /// a compander at all.
    #[test]
    fn a_quiet_signal_is_pushed_up_the_line() {
        let mut c = Compander::new(FS);
        let (mid, _) = settle(&mut c, 0.02, 0.5);
        assert!(mid > 0.02 * 4.0, "0.02 only reached {mid}");
        assert!(mid <= COMPRESSOR_BOUND);
    }

    /// A step out of silence must not be unbounded, and must not be silent either — the overshoot
    /// is real and is bounded rather than removed.
    #[test]
    fn a_step_from_silence_is_bounded() {
        let mut c = Compander::new(FS);
        let mut worst = 0.0f32;
        for i in 0..(FS as usize) {
            let x = if i < 10 { 0.0 } else { 1.0 };
            let mid = c.compress(x);
            let out = c.expand(mid);
            assert!(mid.is_finite() && out.is_finite());
            assert!(mid.abs() <= COMPRESSOR_BOUND);
            worst = worst.max(out.abs());
        }
        assert!(worst.is_finite() && worst < 8.0, "{worst}");
    }

    /// A non-finite sample is a zero at either half. `f32::max` ignores a NaN, so a detector
    /// holding one reports its floor for ever: the compressor stuck at 60 dB of gain and the
    /// expander at −60 dB, finite and wrong.
    #[test]
    fn a_non_finite_sample_cannot_poison_either_detector() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut poisoned = Compander::new(FS);
            let mut reference = Compander::new(FS);
            for i in 0..(0.2 * FS) as usize {
                let x = 0.5 * (core::f32::consts::TAU * 220.0 * i as f32 / FS).sin();
                let at = i == 100;
                let p_mid = poisoned.compress(if at { bad } else { x });
                let r_mid = reference.compress(if at { 0.0 } else { x });
                let p_out = poisoned.expand(if at { bad } else { p_mid });
                let r_out = reference.expand(if at { 0.0 } else { r_mid });
                assert!(
                    p_mid.to_bits() == r_mid.to_bits() && p_out.to_bits() == r_out.to_bits(),
                    "{bad} at sample {i}: {p_mid} / {p_out} against {r_mid} / {r_out}"
                );
            }
        }
    }

    #[test]
    fn silence_stays_exactly_silent_and_reset_leaves_nothing() {
        let mut c = Compander::new(FS);
        for _ in 0..1000 {
            let mid = c.compress(0.0);
            assert_eq!(c.expand(mid), 0.0);
        }
        for i in 0..1000 {
            let mid = c.compress(if i < 500 { 0.7 } else { 0.0 });
            let _ = c.expand(mid);
        }
        c.reset();
        assert_eq!(c.compressor_average.state, 0.0);
        assert_eq!(c.expander.average.state, 0.0);
    }
}
