//! The bucket brigade device, and the circuit around it — as plain Rust.
//!
//! The device is `research:effects/bucket-brigade-delay.md`; the product is
//! `plans/plan-mxm-bucket-delay.md`. Two published models are followed, in the places each is
//! better, and **both are implemented from the papers' equations**: no code from either author, and
//! none from any third-party implementation, is used here. Neither paper carries a licence grant,
//! which is why only the method is taken — the plan's §8 records that gate.
//!
//! - **The line and its resampling follow Holters & Parker**, *A Combined Model for a Bucket
//!   Brigade Device and its Input and Output Filters*, Proc. DAFx-18, Aveiro, 2018. A fixed-length
//!   line at its own clock rate, with the conversion between the host rate and the clock rate done
//!   by the circuit's own input and output filters rather than by an interpolator ([`modal`],
//!   [`bbd`]).
//! - **The components follow Raffel & Smith**, *Practical Modeling of Bucket-Brigade Device
//!   Circuits*, Proc. DAFx-10, Graz, 2010: the compander ([`compander`]), the filter topology, and
//!   the polynomial nonlinearity ([`bbd::nonlinearity`]).
//!
//! # The thesis, because it decides the architecture
//!
//! In a digital delay, time, brightness, noise and grit are four independent choices. In a bucket
//! brigade they are **one**, because all four follow from the clock: the only way to lengthen the
//! delay is to slow the sampler. Every type here exists to keep that coupling intact. Nothing in
//! this crate lets any of the four be set independently of [`Clock`].
//!
//! # What is read off a document, and what is chosen
//!
//! The collection's rule is that a constant nobody measured says so. Read off Panasonic's own
//! catalogue: the stage counts, the tap positions ([`bbd::MN3011_TAP_STAGES`]), the delay law, the
//! per-part THD and S/N figures ([`Line::thd_typical`], [`Line::signal_to_noise_db`]), and the
//! 2 kHz cutoff of both reference echo circuits ([`FIXED_CUTOFF_HZ`]). Taken from the two papers:
//! the polynomial's coefficients and the compander's structure. **Chosen** — and each says so where
//! it is defined: the filters' Butterworth alignment ([`modal`] explains why the catalogue's own R
//! and C values could not be used), the noise colour, the compander's time constant, and the
//! tracking filter's ratio.

#![forbid(unsafe_code)]

pub mod bbd;
pub mod compander;
pub mod modal;
pub mod unit;

pub use bbd::{Bbd, Line};
pub use compander::Compander;
pub use unit::{Controls, Core, Return, Routing, Shape, Unit, applied_feedback, effective_taps};

/// Flush a denormal to exact zero.
///
/// Every recursive state in this crate passes through here. Not a framework FTZ guard — that may be
/// a no-op without an opt-in feature, and DSP-level flushing is what preserves exact digital
/// silence, which this product needs because a finite tail has to *reach* zero for the host to be
/// told the tail ended (the plan's §3).
///
/// `1e-20` is far above the f32 denormal threshold (~1.18e-38) and about -400 dB, so nothing
/// audible is lost.
#[inline(always)]
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 { 0.0 } else { x }
}

/// A non-finite sample becomes zero; everything else passes untouched.
///
/// **At the entry of every seam a sample arrives through**, not at the output: a NaN that reached
/// the line, a filter's state or a detector stays there, because every lap re-reads it, and a
/// sanitised output would only hide a delay that had died. [`flush`] deliberately does not do this,
/// so that a NaN made *inside* the model still shows in every finiteness test.
#[inline(always)]
pub(crate) fn finite_or_zero(x: f32) -> f32 {
    if x.is_finite() { x } else { 0.0 }
}

/// Small xorshift PRNG. Allocation-free, deterministic, and seeded explicitly so every noise source
/// is bit-repeatable for a given seed — which is what lets the noise floor be *measured* per line
/// (the plan's §7 *longer lines are worse, in the right order*) rather than eyeballed.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u32,
}

impl Rng {
    pub const fn new(seed: u32) -> Self {
        // A zero state is a fixed point for xorshift, so forbid it.
        Self {
            state: if seed == 0 { 0x9E37_79B9 } else { seed },
        }
    }

    /// Next uniform sample in `[-1, 1)`.
    #[inline]
    pub fn next_bipolar(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        // Map the top 24 bits into [-1, 1) so the result is exactly representable.
        ((self.state >> 8) as f32 / 8_388_608.0) - 1.0
    }
}

/// The fixed filters' cutoff, in Hz.
///
/// **Read off the catalogue, not chosen**: both of Panasonic's reference echo circuits state
/// `f_co = 2 kHz` (pp. 58, 59), which for the MN3005 at an 18 kHz clock is under a ninth of the
/// clock rather than the third Raffel & Smith give as typical. It is a *worst-case fixed filter*,
/// picked for the longest delay the circuit reaches, and it is why a short delay in such a box is
/// no brighter than a long one — the filter does not know.
pub const FIXED_CUTOFF_HZ: f64 = 2000.0;

/// The tracking filters' cutoff as a fraction of the clock.
///
/// **Chosen.** Raffel & Smith give a switched-capacitor BBD filter's cutoff as *"typically chosen
/// to be between ⅓ and ½ of the sampling frequency"*; this takes the dark end of that range, so
/// `Tracking` stays a plausible member of the same family as `Fixed` rather than becoming a
/// different, brighter product. The bench measurement that would replace it is an alias floor at
/// the shortest delay, where the ratio decides how much of the input's top end reaches the line.
pub const TRACKING_CUTOFF_RATIO: f64 = 1.0 / 3.0;

/// The clock range the MN3101 reaches with the catalogue's own component examples (pp. 56–61):
/// 0.7 kHz at the slow end of its slowest example, 750 kHz at the fast end of its fastest.
///
/// Clamped here rather than only in the parameter layer, which is the DSP contract's rule: a
/// modulation sum can drive a control past its own range, and `Wobble` is exactly such a sum.
pub const CLOCK_MIN_HZ: f64 = 700.0;
/// See [`CLOCK_MIN_HZ`].
pub const CLOCK_MAX_HZ: f64 = 750_000.0;

/// The clock, which is the only time control there is.
///
/// The delay law is `t_delay = N / (2 · f_CP)` — the catalogue's own graph (p. 1) and Raffel &
/// Smith §2.1, checked in the research page against Panasonic's two reference circuits, whose six
/// stated bounds all agree. Everything else about the sound follows: the Nyquist limit falls with
/// the delay, changing the delay bends pitch because the samples already in the line leave at the
/// new rate, and the device's own bandwidth is `0.3 · f_CP`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clock {
    hz: f64,
}

impl Clock {
    /// A clock at `hz`, clamped to the range the hardware's own driver reaches.
    #[inline]
    pub fn from_hz(hz: f64) -> Self {
        Self {
            hz: if hz.is_finite() {
                hz.clamp(CLOCK_MIN_HZ, CLOCK_MAX_HZ)
            } else {
                CLOCK_MIN_HZ
            },
        }
    }

    /// The clock that puts `stages` at `seconds` of delay: `f_CP = N / (2 · t)`.
    #[inline]
    pub fn for_delay(stages: usize, seconds: f64) -> Self {
        Self::from_hz(if seconds > 0.0 {
            stages as f64 / (2.0 * seconds)
        } else {
            CLOCK_MAX_HZ
        })
    }

    #[inline]
    pub fn hz(self) -> f64 {
        self.hz
    }

    /// Seconds per clock period. The sample-and-hold holds each output sample for exactly this
    /// long, which is where the device's high-frequency roll-off comes from.
    #[inline]
    pub fn period(self) -> f64 {
        1.0 / self.hz
    }

    /// The delay a line of `stages` gives at this clock, in seconds.
    #[inline]
    pub fn delay_seconds(self, stages: usize) -> f64 {
        stages as f64 / (2.0 * self.hz)
    }

    /// The device's own input bandwidth limit, `f_i ≤ 0.3 · f_CP` — Panasonic specify it for the
    /// MN3001/3002/3003 (pp. 3, 9), and it sits well inside the line's own Nyquist of `f_CP / 2`.
    #[inline]
    pub fn device_bandwidth_hz(self) -> f64 {
        0.3 * self.hz
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The delay law against Panasonic's own two reference echo circuits (research page §2). Both
    /// state a clock with a tolerance and a measured typical, maximum and minimum delay, and all
    /// six bounds have to fall out of `N / (2 · f_CP)` — the only independent check on the law this
    /// repository can make without hardware.
    #[test]
    fn the_delay_law_reproduces_both_of_panasonics_reference_circuits() {
        // MN3005 echo (catalogue p. 58): 4096 stages, 18 ± 2 kHz, t_D typ 113, max 128, min 100 ms.
        let ms = |hz: f64| Clock::from_hz(hz).delay_seconds(4096) * 1e3;
        assert!((ms(18_000.0) - 113.0).abs() < 1.0, "typ {}", ms(18_000.0));
        assert!((ms(16_000.0) - 128.0).abs() < 1.0, "max {}", ms(16_000.0));
        assert!((ms(20_000.0) - 100.0).abs() < 3.0, "min {}", ms(20_000.0));

        // MN3007 echo (p. 59): 1024 stages, 14 ± 2 kHz, t_D typ 37, max 43, min 32 ms.
        let ms = |hz: f64| Clock::from_hz(hz).delay_seconds(1024) * 1e3;
        assert!((ms(14_000.0) - 37.0).abs() < 1.0, "typ {}", ms(14_000.0));
        assert!((ms(12_000.0) - 43.0).abs() < 1.0, "max {}", ms(12_000.0));
        assert!((ms(16_000.0) - 32.0).abs() < 1.0, "min {}", ms(16_000.0));
    }

    /// The clock is a *rate*, so asking for a longer delay must slow it. One line of arithmetic,
    /// which is exactly why it is worth a test: a sign error here gives a delay whose tone
    /// brightens as it lengthens, which is the one thing a bucket brigade never does.
    #[test]
    fn a_longer_delay_is_a_slower_clock_and_a_narrower_device_bandwidth() {
        let short = Clock::for_delay(4096, 0.050);
        let long = Clock::for_delay(4096, 0.400);
        assert!(long.hz() < short.hz());
        assert!(long.device_bandwidth_hz() < short.device_bandwidth_hz());

        // Raffel & Smith's own worked case: 300 ms from 4096 stages needs 6827 Hz of clock, so
        // nothing above 3413 Hz may enter without aliasing.
        let worked = Clock::for_delay(4096, 0.300);
        assert!((worked.hz() - 6826.7).abs() < 1.0, "{}", worked.hz());
        assert!((worked.hz() / 2.0 - 3413.3).abs() < 1.0);
    }

    #[test]
    fn the_clock_survives_a_modulation_sum_driving_it_out_of_range() {
        assert_eq!(Clock::from_hz(f64::NAN).hz(), CLOCK_MIN_HZ);
        assert_eq!(Clock::from_hz(-1.0).hz(), CLOCK_MIN_HZ);
        assert_eq!(Clock::from_hz(1e12).hz(), CLOCK_MAX_HZ);
        assert_eq!(Clock::for_delay(4096, 0.0).hz(), CLOCK_MAX_HZ);
    }

    #[test]
    fn flush_preserves_audible_values_and_zeroes_denormals() {
        assert_eq!(flush(0.0), 0.0);
        assert_eq!(flush(1e-30), 0.0);
        assert_eq!(flush(-1e-30), 0.0);
        assert_eq!(flush(0.5), 0.5);
        assert_eq!(flush(-1e-6), -1e-6);
    }

    #[test]
    fn the_rng_is_bipolar_bounded_and_repeatable() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(1);
        for _ in 0..10_000 {
            let x = a.next_bipolar();
            assert_eq!(x, b.next_bipolar());
            assert!((-1.0..1.0).contains(&x));
        }
    }
}
