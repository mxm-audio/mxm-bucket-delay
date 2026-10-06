//! T12 — mxm-bucket-delay auditioned through the player's own effect chain.
//!
//! The product plan's §7 has a row that no unit test can discharge: *loaded into the FX chain,
//! audible, editor opens, and **bypass restores the dry** — not silence; only the effect's
//! contribution goes.* This is that row, run through the same path a person's audio takes:
//! `mxm-mono-01` into the chain, the delay after it, the worker driven by hand.
//!
//! # Why it is not compared to a reference the way T11 is
//!
//! T11's fixture is arithmetic this repository can repeat, so its chain is proved to the bit. This
//! effect is a bucket brigade with an eighth-order filter chain, a compander and a nonlinearity in
//! its loop — restating that here would be a second implementation, free to agree with the first
//! about the wrong thing. What is asserted here is what the chain is *for*: that the effect is
//! reached, that what comes back is the delay it claims to be, that switching it off returns the
//! dry untouched, and that the graph goes to exact silence afterwards rather than idling for ever.
//!
//! The DSP's own behaviour — the delay law, the taps, the pitch bend, the singing threshold — is
//! measured where it can be measured, in `crates/mxm-bucket-delay-dsp`.

use mxm_player_harness::harness;

use harness::{CHANNELS, Harness, mxm_mono_01};
use mxm_player::events::input::Payload;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

const SOURCE: &str = "dk.mxm.mxm-mono-01";
const EFFECT: &str = "dk.mxm.mxm-bucket-delay";
const GUI: usize = 0;
const BLOCK: usize = 256;
/// Long enough for the note, its release, the delay's repeats and the silence after them.
const BLOCKS: usize = 900;
const SAMPLE_RATE: f32 = 48_000.0;

/// The plugin's own default delay, in seconds. Restated so a change there fails here by name.
const DEFAULT_TIME_S: f32 = 0.25;

/// Where the delay's bundle lives, or `None` if it has not been built.
fn bucket_delay() -> Option<PathBuf> {
    let path = mxm_player_harness::workspace_root().join("target/bundled/mxm-bucket-delay.clap");
    if path.exists() {
        Some(path)
    } else {
        eprintln!(
            "skipping: {} is missing — run `cargo xtask bundle mxm-bucket-delay --release`",
            path.display()
        );
        None
    }
}

fn channels_of(interleaved: &[f32]) -> Vec<Vec<f32>> {
    (0..CHANNELS)
        .map(|c| interleaved.chunks(CHANNELS).map(|frame| frame[c]).collect())
        .collect()
}

fn last_sounding_frame(channels: &[Vec<f32>]) -> Option<usize> {
    let frames = channels.first().map(Vec::len).unwrap_or(0);
    (0..frames)
        .rev()
        .find(|&f| channels.iter().any(|c| c[f] != 0.0))
}

#[derive(Copy, Clone)]
enum Act {
    NoteOn(u8),
    NoteOff(u8),
    /// Switch the effect off (`true`) or on, exactly as the strip does.
    Bypass(bool),
}

/// The note every test plays: on at block 2, off at block 30 — short, so the repeats that follow
/// it are unmistakably the effect's rather than the source's release.
const NOTE: &[(usize, Act)] = &[(2, Act::NoteOn(60)), (30, Act::NoteOff(60))];

fn payload(act: Act) -> Option<Payload> {
    match act {
        Act::NoteOn(key) => Some(Payload::NoteOn {
            channel: 0,
            key,
            velocity: 100.0 / 127.0,
        }),
        Act::NoteOff(key) => Some(Payload::NoteOff {
            channel: 0,
            key,
            velocity: 0.0,
        }),
        Act::Bypass(_) => None,
    }
}

/// Runs the script through the source, with the delay in the chain when `with_effect`.
fn run(with_effect: bool, script: &[(usize, Act)]) -> Option<Vec<f32>> {
    let source = mxm_mono_01()?;
    let effect = bucket_delay()?;
    let chain: Vec<(&Path, &str)> = if with_effect {
        vec![(effect.as_path(), EFFECT)]
    } else {
        vec![]
    };
    let mut h = Harness::with_fx(&source, SOURCE, 1, &chain).expect("the chain builds");

    let mut out = Vec::with_capacity(BLOCKS * BLOCK * CHANNELS);
    for block in 0..BLOCKS {
        for (at, act) in script {
            if *at != block {
                continue;
            }
            match act {
                Act::Bypass(off) => {
                    if let Some(fx) = h.fx.first() {
                        fx.bypassed.store(*off, Ordering::Relaxed);
                    }
                }
                other => {
                    let payload = payload(*other).expect("an event");
                    // Stamped at time zero: it lands at frame 0 of this block, every run alike.
                    assert!(h.push_at(GUI, 0, payload), "the queue accepts the event");
                }
            }
        }
        out.extend_from_slice(h.render(BLOCK));
    }
    h.shutdown();
    Some(out)
}

/// Peak magnitude over a frame range, across both channels.
fn peak(channels: &[Vec<f32>], range: std::ops::Range<usize>) -> f32 {
    channels
        .iter()
        .flat_map(|c| c[range.clone()].iter())
        .fold(0.0f32, |m, &s| m.max(s.abs()))
}

/// **The player hosts it, and what comes back is a delay.**
///
/// The source's note ends; the delay's repeats do not. What is asserted is the arithmetic of the
/// product's one law rather than the effect's tone: something arrives about one delay time after
/// the source stopped, and nothing arrives before the source started.
#[test]
fn the_delay_is_reached_and_what_comes_back_arrives_a_delay_time_later() {
    let Some(dry) = run(false, NOTE) else {
        return;
    };
    let dry = channels_of(&dry);
    let wet = channels_of(&run(true, NOTE).expect("built above"));

    let dry_end = last_sounding_frame(&dry).expect("the source sounded");
    let wet_end = last_sounding_frame(&wet).expect("the chain sounded");
    assert!(
        wet_end > dry_end,
        "the delay's repeats must outlive the source: dry ended at {dry_end}, wet at {wet_end}"
    );

    // A repeat lands about one delay time after the source stopped. The window is generous on
    // purpose: the filters in the circuit have their own group delay, and the point of the
    // assertion is *that a repeat is there*, not where to the sample.
    let delay = (DEFAULT_TIME_S * SAMPLE_RATE) as usize;
    let after = dry_end + delay / 2..dry_end + delay * 2;
    assert!(
        peak(&wet, after.clone()) > 1e-4,
        "nothing came back between frames {}..{} — a delay that delays nothing is not in the chain",
        after.start,
        after.end
    );

    // And nothing is added before the source ever sounded: an effect that leaked would show here.
    let first_sound = (0..dry[0].len())
        .find(|&f| dry.iter().any(|c| c[f] != 0.0))
        .expect("the source sounded");
    assert_eq!(
        peak(&wet, 0..first_sound),
        0.0,
        "the chain produced something before the source did"
    );
}

/// **Bypass restores the dry — not silence.** Only the effect's contribution goes, which is the
/// distinction the plan draws and the one a chain gets wrong.
#[test]
fn switching_the_effect_off_returns_the_dry_signal_to_the_bit() {
    let Some(dry) = run(false, NOTE) else {
        return;
    };
    let mut script = vec![(0, Act::Bypass(true))];
    script.extend_from_slice(NOTE);
    let bypassed = run(true, &script).expect("built above");

    assert_eq!(
        bypassed, dry,
        "an effect that is off must not touch the signal, sample for sample"
    );
    // The test bites only if the dry is actually audible — a comparison of two silences passes for
    // the wrong reason, which is the mistake `mxm-folded-spring`'s own notes warn about.
    assert!(
        peak(&channels_of(&dry), 0..dry.len() / CHANNELS) > 1e-3,
        "the source must be audible for this comparison to mean anything"
    );
}

/// **The graph sleeps.** The repeats end, the loop snaps to exact zero, and the output is digital
/// silence rather than a residual that keeps a host awake for ever — which is the collection's
/// rule that an effect doing nothing uses no CPU, seen from the host's side.
#[test]
fn once_the_repeats_have_died_the_output_is_exact_silence() {
    let Some(wet) = run(true, NOTE) else {
        return;
    };
    let wet = channels_of(&wet);
    let end = BLOCKS * BLOCK;
    let last = last_sounding_frame(&wet).expect("the chain sounded");
    assert!(
        last < end - 4 * BLOCK,
        "the repeats were still sounding at frame {last} of {end}; the run is too short to prove \
         anything about silence"
    );
    assert!(
        wet.iter()
            .all(|c| c[end - 2 * BLOCK..].iter().all(|&s| s == 0.0)),
        "the chain must reach exact silence, not a decaying residual"
    );
}

/// An effect switched back on after the source has gone quiet must not replay what it was holding
/// when it went off. The delay empties rather than freezes, so there is nothing stale to spill.
#[test]
fn an_effect_switched_back_on_starts_from_silence() {
    let Some(dry) = run(false, NOTE) else {
        return;
    };
    let dry = channels_of(&dry);
    let dry_end = last_sounding_frame(&dry).expect("the source sounded");

    let resume = 600;
    assert!(
        dry_end < resume * BLOCK,
        "the source must be silent by block {resume}; it sounded until frame {dry_end}"
    );
    let mut script = NOTE.to_vec();
    // Off while the line still holds the note, back on long after the source is silent.
    script.push((10, Act::Bypass(true)));
    script.push((resume, Act::Bypass(false)));
    let wet = channels_of(&run(true, &script).expect("built above"));

    assert!(
        wet.iter()
            .all(|c| c[resume * BLOCK..].iter().all(|&s| s == 0.0)),
        "an effect coming back on must not play what it held before it was switched off"
    );
    assert!(
        dry.iter()
            .any(|c| c[..10 * BLOCK].iter().any(|&s| s != 0.0)),
        "the source was sounding when the effect was switched off"
    );
}
