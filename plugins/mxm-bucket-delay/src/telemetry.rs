//! The **only** channel from the audio thread to the editor.
//!
//! Atomics, written once per block, read whenever the editor happens to look. No locks, no
//! allocation, and the UI may drop as many frames as it likes — a display that made the audio
//! thread wait would be a display that could cause a dropout.
//!
//! Two rules carried from every other `telemetry.rs` in the collection:
//!
//! - **A peak is max-combined and reset when the UI reads it.** Overwriting each block means a
//!   transient that landed between two frames is simply gone.
//! - **A clip latches until acknowledged.** Design system §5.4.
//!
//! # The ring is what the constellation is drawn from
//!
//! The display's six marks brighten with **the wet the audio thread actually added** — after the
//! level and after any transition fade — not with a guess made in the editor from the control
//! values. So a line that has snapped to silence goes dark, and a `Line` switched mid-tail visibly
//! dims through the change, which is the one thing no knob position can tell you.
//!
//! # No developer channel
//!
//! It arrives as MIDI CC and an effect has no note port; `plugins/AGENTS.md` records why that is a
//! statement about effects rather than an omission here.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[derive(Debug)]
pub struct Telemetry {
    /// Peak of the samples produced, max-combined, reset on read.
    peak: AtomicU32,
    /// Sticky: set when a sample reaches full scale, cleared only by the user.
    clipped: AtomicBool,
    /// The loudest wet the block added, max-combined, reset on read.
    ring: AtomicU32,
    /// The host's tempo, or zero where the host gives none. **Not a meter**: it is overwritten
    /// each block and never cleared on read, because the editor wants the tempo in force and not
    /// the loudest tempo since it last looked.
    ///
    /// Here because `Time` selects a *subdivision* when `Sync` is following the host, and the panel
    /// has to be able to name it. A tempo reaches a plugin only inside `process`, so without this
    /// channel the one control whose meaning depends on the transport has nothing to say.
    tempo: mxm_tempo::TempoCell,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            peak: AtomicU32::new(0),
            clipped: AtomicBool::new(false),
            ring: AtomicU32::new(0),
            tempo: mxm_tempo::TempoCell::new(),
        }
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    fn publish_max(slot: &AtomicU32, value: f32) {
        let bits = value.abs().to_bits();
        let mut current = slot.load(Ordering::Relaxed);
        loop {
            if f32::from_bits(current) >= value.abs() {
                return;
            }
            match slot.compare_exchange_weak(current, bits, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return,
                Err(seen) => current = seen,
            }
        }
    }

    pub fn publish_peak(&self, peak: f32) {
        Self::publish_max(&self.peak, peak);
        if peak >= 1.0 {
            self.clipped.store(true, Ordering::Relaxed);
        }
    }

    pub fn publish_ring(&self, ring: f32) {
        Self::publish_max(&self.ring, ring);
    }

    /// Reads and clears, so a transient between two frames is reported once rather than lost.
    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak.swap(0, Ordering::Relaxed))
    }

    pub fn take_ring(&self) -> f32 {
        f32::from_bits(self.ring.swap(0, Ordering::Relaxed))
    }

    /// The host's tempo this block, or `None` where it gave none.
    pub fn publish_tempo(&self, bpm: Option<f64>) {
        self.tempo.publish(bpm);
    }

    /// The tempo the audio thread last saw. `None` until a block has run, and in a host with no
    /// transport.
    pub fn tempo(&self) -> Option<f64> {
        self.tempo.get()
    }

    pub fn clipped(&self) -> bool {
        self.clipped.load(Ordering::Relaxed)
    }

    pub fn clear_clip(&self) {
        self.clipped.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peak_is_max_combined_and_cleared_on_read() {
        let t = Telemetry::new();
        t.publish_peak(0.2);
        t.publish_peak(0.7);
        t.publish_peak(0.4);
        assert_eq!(
            t.take_peak(),
            0.7,
            "the loudest of the block has to survive"
        );
        assert_eq!(t.take_peak(), 0.0, "and reading has to clear it");
    }

    #[test]
    fn a_clip_latches_until_it_is_acknowledged() {
        let t = Telemetry::new();
        t.publish_peak(1.0);
        assert!(t.clipped());
        t.take_peak();
        assert!(t.clipped(), "reading the peak must not clear the clip");
        t.clear_clip();
        assert!(!t.clipped());
    }

    /// The tempo is the one in force, not a peak: it overwrites, and reading does not clear it.
    #[test]
    fn the_tempo_is_the_one_in_force_rather_than_a_meter() {
        let t = Telemetry::new();
        assert_eq!(t.tempo(), None, "no block has run yet");
        t.publish_tempo(Some(120.0));
        t.publish_tempo(Some(90.0));
        assert_eq!(t.tempo(), Some(90.0));
        assert_eq!(t.tempo(), Some(90.0), "reading it must not clear it");
        t.publish_tempo(None);
        assert_eq!(
            t.tempo(),
            None,
            "a host that stopped giving one is not 90 bpm"
        );
    }

    #[test]
    fn the_ring_is_its_own_channel() {
        let t = Telemetry::new();
        t.publish_ring(0.5);
        t.publish_peak(0.9);
        assert_eq!(t.take_ring(), 0.5);
        assert_eq!(t.take_peak(), 0.9);
    }
}
