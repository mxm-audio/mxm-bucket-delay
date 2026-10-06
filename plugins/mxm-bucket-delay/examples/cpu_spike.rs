//! What one block costs, against the time the block has.
//!
//! ```bash
//! cargo run -p mxm-bucket-delay --release --example cpu_spike
//! ```
//!
//! Reported 2026-09-06: the player disappeared while the tap faders were being moved. The
//! interface's *Restart player* button — the one shown when the engine is **wedged**, meaning a
//! plugin did not return from `process` in time — spawns a fresh player and exits the old one with
//! code 0, which is exactly the trace that was left: no panic, no Windows fault, a new process.
//!
//! So the question is not "does it crash" but **"can it be too slow"**, and that is measurable. A
//! block of `n` frames at `fs` has `n / fs` seconds to be produced in. Anything approaching that is
//! a dropout; anything past it is a wedge.
//!
//! The settings below are chosen to be the expensive ones rather than the usual ones: the tracking
//! filter redesigns itself whenever the clock moves, the wobble moves the clock every sample, and a
//! fast clock on a long line puts many bucket-brigade ticks inside every host sample.

use mxm_bucket_delay::MxmBucketDelay;
use nice_plug::params::{InternalParamMut, Param};
use std::time::Instant;

const FS: f32 = 48_000.0;

fn set(plugin: &MxmBucketDelay, id: &str, plain: f32) {
    let p = &plugin.params;
    unsafe {
        match id {
            "time" => {
                let v = p.time.preview_normalized(plain);
                let _ = p.time._internal_set_normalized_value(v);
                p.time._internal_update_smoother(FS, true);
            }
            "mix" => {
                let v = p.mix.preview_normalized(plain);
                let _ = p.mix._internal_set_normalized_value(v);
                p.mix._internal_update_smoother(FS, true);
            }
            "feedback" => {
                let v = p.feedback.preview_normalized(plain);
                let _ = p.feedback._internal_set_normalized_value(v);
                p.feedback._internal_update_smoother(FS, true);
            }
            "wobble" => {
                let v = p.wobble.preview_normalized(plain);
                let _ = p.wobble._internal_set_normalized_value(v);
                p.wobble._internal_update_smoother(FS, true);
            }
            "taps" => {
                for param in [&p.tap1, &p.tap2, &p.tap3, &p.tap4, &p.tap5, &p.tap6] {
                    let v = param.preview_normalized(plain);
                    let _ = param._internal_set_normalized_value(v);
                    param._internal_update_smoother(FS, true);
                }
            }
            other => unreachable!("{other}"),
        }
    }
}

fn set_enum<T: nice_plug::params::enums::Enum + PartialEq + Copy>(
    param: &nice_plug::prelude::EnumParam<T>,
    value: T,
) {
    unsafe {
        let v = param.preview_normalized(value);
        let _ = param._internal_set_normalized_value(v);
    }
}

/// Runs `blocks` blocks and returns the worst and mean cost as a fraction of the block's own time.
fn measure(plugin: &mut MxmBucketDelay, block: usize, blocks: usize) -> (f64, f64) {
    let budget = block as f64 / f64::from(FS);
    let mut worst = 0.0f64;
    let mut total = 0.0f64;
    for i in 0..blocks {
        let mut left: Vec<f32> = (0..block)
            .map(|n| 0.6 * (core::f32::consts::TAU * 220.0 * n as f32 / FS).sin())
            .collect();
        let mut right = vec![0.0f32; block];
        let start = Instant::now();
        {
            let mut refs: Vec<&mut [f32]> = vec![&mut left, &mut right];
            plugin.process_block_for_test(&mut refs, Some(120.0));
        }
        let taken = start.elapsed().as_secs_f64();
        // The first few blocks pay for caches and page faults, which is not what is being asked.
        if i > 8 {
            worst = worst.max(taken / budget);
            total += taken / budget;
        }
    }
    (worst, total / (blocks - 9) as f64)
}

fn main() {
    println!("mxm-bucket-delay — what a block costs against the time it has");
    println!("{FS} Hz. 1.00 means the block took exactly as long as it lasts.\n");
    println!("| Setting | Block | Mean | Worst |");
    println!("|---|---|---|---|");

    for &block in &[64usize, 256, 1024] {
        for (name, time, filter, wobble) in [
            (
                "default (250 ms, Fixed)",
                0.25,
                mxm_bucket_delay::params::FilterChoice::Fixed,
                0.0,
            ),
            (
                "short time (20 ms, Fixed)",
                0.02,
                mxm_bucket_delay::params::FilterChoice::Fixed,
                0.0,
            ),
            (
                "Tracking, still",
                0.25,
                mxm_bucket_delay::params::FilterChoice::Tracking,
                0.0,
            ),
            (
                "Tracking + wobble",
                0.25,
                mxm_bucket_delay::params::FilterChoice::Tracking,
                1.0,
            ),
            (
                "short + Tracking + wobble",
                0.02,
                mxm_bucket_delay::params::FilterChoice::Tracking,
                1.0,
            ),
        ] {
            let mut plugin = MxmBucketDelay::default();
            plugin.prepare_for_test(FS, 1);
            set(&plugin, "mix", 1.0);
            set(&plugin, "feedback", 0.6);
            set(&plugin, "taps", 1.0);
            set(&plugin, "time", time);
            set(&plugin, "wobble", wobble);
            set_enum(&plugin.params.filter, filter);
            set_enum(
                &plugin.params.line,
                mxm_bucket_delay::params::LineChoice::S8192,
            );
            let (worst, mean) = measure(&mut plugin, block, 200);
            println!("| {name} | {block} | {mean:.3} | {worst:.3} |");
        }
    }

    println!("\nAnything at or over 1.00 is a dropout; sustained, it is what the player calls");
    println!("wedged. The 8192-stage line is used throughout: it is the most expensive one.");
}
