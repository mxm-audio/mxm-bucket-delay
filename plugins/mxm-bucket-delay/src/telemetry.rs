//! The **only** channel from the audio thread to the editor.
//!
//! Atomics, written once per block, read whenever the editor happens to look. No locks, no
//! allocation, and the UI may drop as many frames as it likes — a display that made the audio
//! thread wait would be a display that could cause a dropout.
//!
//! **The meter is `mxm_telemetry`'s**, held as `meter`, with one policy for every plugin: a peak
//! is max-combined and reset when the UI reads it, a clip latches until acknowledged (design
//! system §5.4), and a non-finite peak latches the clip and is never the level. What follows here
//! is this plugin's own.
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
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
pub struct Telemetry {
    /// The output level and its clip latch: the collection's one meter policy
    /// (`mxm_telemetry::PeakMeter`).
    pub meter: mxm_telemetry::PeakMeter,
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
            meter: mxm_telemetry::PeakMeter::new(),
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

    pub fn publish_ring(&self, ring: f32) {
        Self::publish_max(&self.ring, ring);
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
        t.meter.publish(0.9);
        assert_eq!(t.take_ring(), 0.5);
        assert_eq!(t.meter.take(), 0.9);
    }
}
