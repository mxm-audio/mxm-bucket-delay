//! The circuit's own filters, as continuous-time systems that can be fed and read at *arbitrary*
//! instants.
//!
//! This module is what makes Holters & Parker's model possible, and it is the reason this product
//! has no interpolator. Their objection to the older approach is that interpolating between the
//! host rate and the clock rate adds filtering and distortion of its own that is not in the
//! circuit — and the filtering it would add sits exactly where the tone lives, in series with the
//! reconstruction filter that is supposed to *be* the tone. Instead the resampling is done by the
//! filters that are in the circuit anyway:
//!
//! - the **anti-alias filter** is fed the host's samples, held between them, and *read at the clock
//!   instants*, where the line samples it;
//! - the **reconstruction filter** is fed the line's held output, which changes at clock instants,
//!   and *read at the host's sample instants*.
//!
//! Neither filter is resampled. Both are advanced by whatever `Δt` the next event happens to be
//! away, which is what this module provides.
//!
//! # How an arbitrary step is exact rather than approximate
//!
//! A continuous-time lowpass with distinct poles is written in partial fractions,
//!
//! ```text
//!   H(s) = Σ  r_k / (s − p_k)
//! ```
//!
//! so its state splits into one independent complex mode per pole, each obeying
//! `dx_k/dt = p_k · x_k + u`. While the input is *constant* that has a closed-form solution:
//!
//! ```text
//!   x_k(t + Δ) = e^{p_k Δ} · x_k(t)  +  u · (e^{p_k Δ} − 1) / p_k
//!   y(t)       = Re Σ r_k · x_k(t)
//! ```
//!
//! No approximation and no step-size limit — the step is exact for *any* `Δ`, which is the whole
//! point. And the input is genuinely piecewise constant on both sides: the host holds its sample
//! for a sample period, and the line's sample-and-hold holds its output for a clock period.
//! Poles come in conjugate pairs with conjugate residues, so the imaginary parts cancel and the
//! output is real; the sum is taken over every mode and its real part kept, which is clearer than
//! folding the pairs by hand and costs a handful of adds.
//!
//! # Why Butterworth, when the catalogue prints the capacitors
//!
//! **The alignment is chosen, and this is the argument for it.** Raffel & Smith give the topology
//! as Sallen-Key, `R = 10 kΩ` throughout, a third-order anti-alias filter in and a third-order plus
//! a second-order correction out — eighth order in total — with typical capacitor values. What they
//! do not give, and what the catalogue's page images do not settle either, is **which capacitor
//! sits at which node**. Worked through for the anti-alias set (6.8 nF, 82 nF, 330 pF) on the
//! standard equal-R third-order unity-gain Sallen-Key, the assignment that looks natural puts a
//! real pole at **16 Hz** — a filter that would remove the instrument, not the aliasing. The values
//! cannot be used without the schematic they came from.
//!
//! So the *orders* and the *cutoff* are taken from the documents, which state both firmly, and the
//! pole placement is a Butterworth alignment because it is the flattest choice that commits to
//! nothing the sources do not say. What would replace it: the reference circuit's schematic read at
//! a resolution that resolves the capacitor designators, or a swept measurement of a real one.
//! Recorded as an open question in the crate's AGENTS.md, not buried here.

use core::ops::{Add, Div, Mul, Sub};

/// The most modes any filter here needs: the reconstruction chain is third order plus second order.
pub const MAX_MODES: usize = 5;

/// A complex number in `f64`.
///
/// Written here rather than pulled in, because this crate has no dependencies and that is what
/// earns its MSRV override. Only the handful of operations the modal update needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C64 {
    pub re: f64,
    pub im: f64,
}

impl C64 {
    #[inline]
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    #[inline]
    pub const fn real(re: f64) -> Self {
        Self { re, im: 0.0 }
    }

    #[inline]
    pub fn norm_sq(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    /// `e^z`, from the polar form of the imaginary part.
    #[inline]
    pub fn exp(self) -> Self {
        let m = self.re.exp();
        Self::new(m * self.im.cos(), m * self.im.sin())
    }

    /// `e^z − 1`, accurate when `|z|` is small.
    ///
    /// The mode update divides this by the pole, and `Δ` is routinely tiny — the sliver of a host
    /// sample left over after the last clock tick. Computing `exp(z) − 1` directly there cancels
    /// the leading 1 and throws away most of the significant digits, which shows up as a filter
    /// that drifts when the clock is fast. The series is used below `|z| = 1e-4`, where its
    /// truncation after the fourth term is far under the f64 epsilon.
    #[inline]
    pub fn expm1(self) -> Self {
        if self.norm_sq() < 1e-8 {
            let z = self;
            let z2 = z * z;
            let z3 = z2 * z;
            let z4 = z2 * z2;
            z + z2 * 0.5 + z3 * (1.0 / 6.0) + z4 * (1.0 / 24.0)
        } else {
            self.exp() - C64::real(1.0)
        }
    }
}

impl Add for C64 {
    type Output = C64;
    #[inline]
    fn add(self, o: C64) -> C64 {
        C64::new(self.re + o.re, self.im + o.im)
    }
}

impl Sub for C64 {
    type Output = C64;
    #[inline]
    fn sub(self, o: C64) -> C64 {
        C64::new(self.re - o.re, self.im - o.im)
    }
}

impl Mul for C64 {
    type Output = C64;
    #[inline]
    fn mul(self, o: C64) -> C64 {
        C64::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}

impl Mul<f64> for C64 {
    type Output = C64;
    #[inline]
    fn mul(self, s: f64) -> C64 {
        C64::new(self.re * s, self.im * s)
    }
}

impl Div for C64 {
    type Output = C64;
    #[inline]
    fn div(self, o: C64) -> C64 {
        let d = o.norm_sq();
        C64::new(
            (self.re * o.re + self.im * o.im) / d,
            (self.im * o.re - self.re * o.im) / d,
        )
    }
}

/// A cascade of Butterworth lowpass sections, in modal form, advanced by arbitrary steps.
///
/// The cutoff is settable in place: `Tracking` moves it with the clock, and nothing here
/// allocates, so that is a per-block assignment rather than a rebuild. See [`Self::set_cutoff`] for
/// what happens to the state when it moves.
#[derive(Debug, Clone)]
pub struct ModalLowpass {
    modes: usize,
    /// Section orders, e.g. `[3]` for the anti-alias filter and `[3, 2]` for the reconstruction
    /// chain. Kept so the cutoff can be moved without being told the shape again.
    orders: [usize; 2],
    sections: usize,
    cutoff_hz: f64,
    poles: [C64; MAX_MODES],
    residues: [C64; MAX_MODES],
    state: [C64; MAX_MODES],
    /// The step the cached propagator was built for. A steady clock repeats the same full period
    /// over and over, so caching it turns most of the per-tick cost into two complex multiplies.
    cached_step: f64,
    propagator: [C64; MAX_MODES],
    input_gain: [C64; MAX_MODES],
}

impl ModalLowpass {
    /// A cascade of Butterworth sections of the given orders, all at one cutoff.
    ///
    /// `orders` is the circuit's, not a design choice: `[3]` in, `[3, 2]` out (Raffel & Smith
    /// §2.2). Orders must sum to at most [`MAX_MODES`].
    pub fn butterworth(orders: &[usize], cutoff_hz: f64) -> Self {
        assert!(!orders.is_empty() && orders.len() <= 2);
        assert!(orders.iter().sum::<usize>() <= MAX_MODES);
        let mut o = [0usize; 2];
        o[..orders.len()].copy_from_slice(orders);
        let mut f = Self {
            modes: orders.iter().sum(),
            orders: o,
            sections: orders.len(),
            cutoff_hz: 0.0,
            poles: [C64::real(0.0); MAX_MODES],
            residues: [C64::real(0.0); MAX_MODES],
            state: [C64::real(0.0); MAX_MODES],
            cached_step: f64::NAN,
            propagator: [C64::real(0.0); MAX_MODES],
            input_gain: [C64::real(0.0); MAX_MODES],
        };
        f.design(cutoff_hz);
        f
    }

    /// Move the cutoff, keeping the output continuous.
    ///
    /// The modal state carries the filter's memory in coordinates that scale with the cutoff: every
    /// residue is proportional to `ω0`, so leaving the state alone while `ω0` moves would step the
    /// output by the same ratio — a click, every block, on a control that is *meant* to be swept.
    /// Scaling the state by the inverse ratio makes the output continuous across the change, which
    /// is what a swept analogue filter does. This is coefficient modulation and is exact only in
    /// the limit of small steps; `Tracking` moves the cutoff with the clock, which is smoothed, so
    /// the steps are small.
    #[inline]
    pub fn set_cutoff(&mut self, cutoff_hz: f64) {
        let target = cutoff_hz.clamp(20.0, 40_000.0);
        if (target - self.cutoff_hz).abs() < 1e-9 {
            return;
        }
        let ratio = self.cutoff_hz / target;
        self.design(target);
        for k in 0..self.modes {
            self.state[k] = self.state[k] * ratio;
        }
    }

    #[inline]
    pub fn cutoff_hz(&self) -> f64 {
        self.cutoff_hz
    }

    fn design(&mut self, cutoff_hz: f64) {
        let w0 = core::f64::consts::TAU * cutoff_hz;
        self.cutoff_hz = cutoff_hz;
        self.cached_step = f64::NAN;

        // Butterworth poles, section by section: for order n they sit on the left half of a circle
        // of radius w0 at angles π(2k + n + 1) / 2n.
        let mut m = 0;
        for s in 0..self.sections {
            let n = self.orders[s];
            for k in 0..n {
                let theta = core::f64::consts::PI * ((2 * k + n + 1) as f64) / (2.0 * n as f64);
                self.poles[m] = C64::new(w0 * theta.cos(), w0 * theta.sin());
                m += 1;
            }
        }

        // Partial fractions over distinct poles: r_k = Num / Π_{j≠k}(p_k − p_j), with the numerator
        // w0^modes, which is what makes the DC gain exactly one.
        let num = C64::real(w0.powi(self.modes as i32));
        for k in 0..self.modes {
            let mut denom = C64::real(1.0);
            for j in 0..self.modes {
                if j != k {
                    denom = denom * (self.poles[k] - self.poles[j]);
                }
            }
            self.residues[k] = num / denom;
        }
    }

    /// Advance by `dt` seconds with the input held at `u` throughout, then leave the state at the
    /// end of the step. Exact for any `dt ≥ 0`.
    #[inline]
    pub fn advance(&mut self, dt: f64, u: f64) {
        if dt <= 0.0 {
            return;
        }
        if dt != self.cached_step {
            self.cached_step = dt;
            for k in 0..self.modes {
                let z = self.poles[k] * dt;
                self.propagator[k] = z.exp();
                self.input_gain[k] = z.expm1() / self.poles[k];
            }
        }
        for k in 0..self.modes {
            self.state[k] = self.propagator[k] * self.state[k] + self.input_gain[k] * u;
        }
    }

    /// The filter's output at the instant the state is currently at.
    #[inline]
    pub fn output(&self) -> f64 {
        let mut y = 0.0;
        for k in 0..self.modes {
            y += self.residues[k].re * self.state[k].re - self.residues[k].im * self.state[k].im;
        }
        y
    }

    /// Clear the memory. `reset()` must leave no tail, and a filter holding a decaying state is a
    /// tail.
    #[inline]
    pub fn reset(&mut self) {
        self.state = [C64::real(0.0); MAX_MODES];
    }

    /// `|H(jω)|` of the designed filter, from the same partial fractions the state update uses.
    ///
    /// Used by the tests to check the running filter against its own design, and by the sinc
    /// measurement to divide the filters out of a measured roll-off. Not called from the audio
    /// path.
    pub fn magnitude(&self, hz: f64) -> f64 {
        let s = C64::new(0.0, core::f64::consts::TAU * hz);
        let mut h = C64::real(0.0);
        for k in 0..self.modes {
            h = h + self.residues[k] / (s - self.poles[k]);
        }
        h.norm_sq().sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The modal update has to agree with the transfer function it was designed from, or the whole
    /// resampling argument rests on a filter that is not the filter. A sine is pushed through at a
    /// fine step and the settled amplitude compared with `|H(jω)|` computed independently.
    #[test]
    fn the_running_filter_matches_its_own_designed_response() {
        for &hz in &[200.0, 1000.0, 2000.0, 4000.0] {
            let mut f = ModalLowpass::butterworth(&[3], 2000.0);
            let fs = 192_000.0;
            let dt = 1.0 / fs;
            let mut peak: f64 = 0.0;
            let n = (fs * 0.05) as usize;
            for i in 0..n {
                let t = i as f64 * dt;
                f.advance(dt, (core::f64::consts::TAU * hz * t).sin());
                // Ignore the first half: the filter has to settle before its amplitude means
                // anything.
                if i > n / 2 {
                    peak = peak.max(f.output().abs());
                }
            }
            let expected = f.magnitude(hz);
            assert!(
                (peak - expected).abs() < 0.02 * expected.max(0.05),
                "{hz} Hz: ran {peak}, designed {expected}"
            );
        }
    }

    /// A third-order Butterworth is down 3 dB at its cutoff and 18 dB per octave above it. If this
    /// drifts, the filter is not the order the circuit is.
    #[test]
    fn the_designed_alignment_is_butterworth() {
        let f = ModalLowpass::butterworth(&[3], 2000.0);
        assert!((f.magnitude(0.0) - 1.0).abs() < 1e-9);
        let at_cutoff = 20.0 * f.magnitude(2000.0).log10();
        assert!(
            (at_cutoff + 3.0103).abs() < 0.01,
            "{at_cutoff} dB at cutoff"
        );
        let octave = 20.0 * (f.magnitude(8000.0) / f.magnitude(4000.0)).log10();
        assert!((octave + 18.06).abs() < 0.1, "{octave} dB per octave");
    }

    /// The whole eighth-order chain, which is what the input actually passes through. Third order
    /// in plus third and second out; 48 dB per octave in the stopband.
    #[test]
    fn the_chain_is_eighth_order_in_total() {
        let anti = ModalLowpass::butterworth(&[3], 2000.0);
        let recon = ModalLowpass::butterworth(&[3, 2], 2000.0);
        let both = |hz: f64| anti.magnitude(hz) * recon.magnitude(hz);
        let octave = 20.0 * (both(16_000.0) / both(8000.0)).log10();
        assert!((octave + 48.16).abs() < 0.2, "{octave} dB per octave");
    }

    /// **The property the resampling rests on.** Splitting a step into two arbitrary pieces with the
    /// input held must land on exactly the state one whole step would — otherwise the answer would
    /// depend on where the clock ticks happened to fall, which is precisely where they do fall.
    #[test]
    fn an_arbitrary_split_step_equals_the_whole_step() {
        let mut whole = ModalLowpass::butterworth(&[3, 2], 2000.0);
        let mut split = whole.clone();
        let dt = 1.0 / 48_000.0;
        for i in 0..2000 {
            let u = ((i as f64) * 0.01).sin();
            whole.advance(dt, u);
            // An awkward, moving split, not a half: the sliver left after a clock tick is never the
            // same length twice when the clock is being swept.
            let a = dt * (0.05 + 0.9 * ((i as f64) * 0.37).sin().abs());
            split.advance(a, u);
            split.advance(dt - a, u);
        }
        assert!(
            (whole.output() - split.output()).abs() < 1e-9,
            "whole {} vs split {}",
            whole.output(),
            split.output()
        );
    }

    /// Very small steps are where `exp(z) − 1` loses its digits, and a fast clock leaves very small
    /// steps every sample. Ten thousand slivers must add up to the same place as the one step they
    /// tile.
    #[test]
    fn a_step_tiled_from_slivers_does_not_drift() {
        let mut whole = ModalLowpass::butterworth(&[3], 2000.0);
        let mut tiled = whole.clone();
        let dt = 1.0 / 48_000.0;
        for i in 0..200 {
            let u = ((i as f64) * 0.05).cos();
            whole.advance(dt, u);
            for _ in 0..50 {
                tiled.advance(dt / 50.0, u);
            }
        }
        assert!(
            (whole.output() - tiled.output()).abs() < 1e-9,
            "whole {} vs tiled {}",
            whole.output(),
            tiled.output()
        );
    }

    /// Moving the cutoff must not step the output — `Tracking` sweeps it with the clock, and a
    /// discontinuity there would be a click on every block.
    #[test]
    fn moving_the_cutoff_keeps_the_output_continuous() {
        let mut f = ModalLowpass::butterworth(&[3, 2], 2000.0);
        let dt = 1.0 / 48_000.0;
        for i in 0..500 {
            f.advance(dt, ((i as f64) * 0.02).sin());
        }
        let before = f.output();
        f.set_cutoff(6000.0);
        assert!(
            (f.output() - before).abs() < 1e-12,
            "{before} then {}",
            f.output()
        );
    }

    #[test]
    fn reset_leaves_no_tail() {
        let mut f = ModalLowpass::butterworth(&[3, 2], 2000.0);
        for _ in 0..500 {
            f.advance(1.0 / 48_000.0, 1.0);
        }
        assert!(f.output().abs() > 0.5);
        f.reset();
        assert_eq!(f.output(), 0.0);
    }

    #[test]
    fn expm1_is_accurate_where_the_naive_form_is_not() {
        // A step small enough that exp(z) − 1 cancels its leading digits.
        let z = C64::new(-1e-9, 3e-9);
        let series = z.expm1();
        // z + z²/2 to f64 precision, computed by hand for the comparison.
        let by_hand = z + z * z * 0.5;
        assert!((series.re - by_hand.re).abs() < 1e-24);
        assert!((series.im - by_hand.im).abs() < 1e-24);
        // And it still agrees with the direct form where the direct form is fine.
        let z = C64::new(-0.7, 1.3);
        let direct = z.exp() - C64::real(1.0);
        assert!((z.expm1().re - direct.re).abs() < 1e-14);
        assert!((z.expm1().im - direct.im).abs() < 1e-14);
    }
}
