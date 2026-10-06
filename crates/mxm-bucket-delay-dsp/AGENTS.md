# AGENTS.md — crates/mxm-bucket-delay-dsp

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The bucket brigade device and the circuit around it, as plain Rust: the clocked line, its six taps,
the filters either side of it, the compander, the nonlinearity and the noise — plus the loop, the
routing and the transitions that make a product out of them. Free of any plugin-framework types, so
the whole signal path is testable with `cargo test` and no host involved.

No instrument sits above it, so no golden digest constrains it. The history, measurements and
reasoning behind each contract are in [NOTES.md](NOTES.md), linked from each section.

# Ownership

Owns `src/` (`lib.rs`, `modal.rs`, `bbd.rs`, `compander.rs`, `unit.rs`), `examples/feedback_spike.rs`,
`examples/feedback_travel.rs` and `Cargo.toml`.

Does **not** own parameter ranges, curves, smoothing or the control mapping — those belong to
[`plugins/mxm-bucket-delay/`](../../plugins/mxm-bucket-delay/AGENTS.md). This crate takes plain
values and a sample rate.

**Named for the product, not the device** (no `mxm-bbd-dsp`): a shared bucket-brigade API waits for
a second honest implementation and two real call sites ([NOTES.md](NOTES.md#why-the-crate-is-named-for-the-product)).

# Local Contracts

## Sources: the papers' equations, nothing else

- **From the papers' equations only.** No code from either author or any third-party
  implementation is used or consulted: none carries a licence grant. The plan's §8 records the gate
  ([NOTES.md](NOTES.md#which-published-model-each-part-follows-and-why-the-two-disagree)).
- **The line and its resampling: Holters & Parker, DAFx-18.** A fixed-length line at its own clock
  rate; the rate conversion is the circuit's own input and output filters, never an interpolator.
- **The components: Raffel & Smith, DAFx-10.** The compander's structure, the filter topology and
  the polynomial nonlinearity.

## The clock and the line

- **`STAGES_PER_CLOCK_PERIOD = 2` stays a named constant**, never a bare `/ 2`. The line is indexed
  in clock periods; a tap `s` stages along comes back `s / 2` periods later.
- **`BUFFER_SLOTS` is `MAX_TICKS + 1`**: the write happens before the taps are read
  ([NOTES.md](NOTES.md#the-two-phase-clock-is-where-the-delay-laws-factor-of-two-lives)).
- **`Line::delay_bounds` is derived, not chosen**: `[N / 2f_max, N / 2f_min]`, the delay law against
  `CLOCK_MIN_HZ` and `CLOCK_MAX_HZ`. The plugin uses it for which tempo subdivisions are reachable
  ([NOTES.md](NOTES.md#linedelay_bounds-is-the-chips-own-reach-and-the-parameter-layer-needs-it)).

## Where the literature is not followed

- **THD is the catalogue's figure** (`Line::thd_typical`), not Raffel & Smith's law.
- **The catalogue's THD order is not monotonic and must not be "fixed".**
  `Line::signal_to_noise_db` *is* monotonic, and the product's premise rests on that order
  ([NOTES.md](NOTES.md#one-number-from-the-literature-is-not-used)).
- **The clamp is taken at the polynomial's own endpoints** (`1 − b`, `−1 + b`), not at the paper's
  clipping cases, which step and are not monotonic
  ([NOTES.md](NOTES.md#one-declared-correction-to-raffel--smith)).
- **The device's input limit is clamped on the input**, at full scale, never on the drive.

## The filters' alignment is chosen

The orders and the cutoff are the documents' (third order in, third plus second out, `f_co = 2 kHz`);
the pole placement is a Butterworth alignment, **chosen** ([NOTES.md](NOTES.md#the-filters-alignment-is-chosen-and-the-reason-is-recorded-rather-than-hidden)).
**Open, and this is where it is recorded:** the reference circuit's schematic read at a resolution
that resolves the capacitor designators, or a swept measurement of a real one, would replace the
alignment with the circuit's own.

## The loop

- **Takeoff after reconstruction, return at the input summing node ahead of the anti-alias
  filter**, so every lap re-applies the whole chain. Never dirt applied once at the output
  ([NOTES.md](NOTES.md#the-loops-takeoff-and-return-nodes-are-read-off-the-circuit)).
- Declared departures: the compander's **position** is ours, and `Return = Tail` gives the takeoff
  **its own reconstruction filter and expander state**.

## Faders set shape, Mix the balance, Feedback regeneration

One control, one meaning ([NOTES.md](NOTES.md#the-faders-set-shape-mix-sets-the-balance-feedback-sets-regeneration)):

- **The mixer normalises by `√Σg²`**: faders are relative weights; opening one keeps the level.
- **The loop divides by `effective_taps`, the participation ratio `(Σg)²/Σg²`**, never a literal
  count, which would step as a fader crossed zero.
- **`Return = Tail` feeds the loop one tap**, so its effective count is one whatever the mixer is
  doing for the output.
- Every line sings at the same place on the travel whatever the taps (`examples/feedback_travel.rs`).
- **Re-fit the law by measurement, never by arithmetic**, and keep the measuring harness free of
  compensation of its own. Both have gone wrong twice.

## `mix` is a crossfade

`out = (1 - mix) * dry + mix * fade * wet` (the owner, 2026-09-06): dry alone at one end, wet alone
at the other ([NOTES.md](NOTES.md#mix-is-a-crossfade-and-the-fade-rides-the-wet-alone)).

- **`mix = 0` is bit-exact dry** (dry gain exactly one): `Fade::Parked` rests on it.
- **The structural fade multiplies the wet term only**; a `Line` or `Routing` change never ducks the dry.
- Tests state their claims at the mix they need: dry below one, wet clean of the dry only at one.

## Feedback is calibrated against the running threshold

- `Line::sings_at` and `Line::starts_at` are **measured** by `examples/feedback_spike.rs`, never
  derived: the compander is inside the loop
  ([NOTES.md](NOTES.md#feedback-is-calibrated-not-derived-and-the-running-threshold-is-the-one-to-use)).
- **Map the control against the running figure**, never the from-silence one. `feedback_gain`
  scales by each line's own threshold, so all four sing at the same place on the knob.

## The snap to exact zero

- A quiet loop is cleared to exact zero once the quiet outlasts **the longest journey anything still
  held can take, plus `SNAP_HOLD_S`**, never on the output alone
  ([NOTES.md § worked examples](NOTES.md#the-snap-to-exact-zero-and-the-one-thing-it-must-yield-to)).
- **The journey is judged over the quiet, never at the controls now**: the longest lap either line
  has actually run at since the last loud sample (`Wobble` included), plus, while `Reverse` is in,
  twice the larger of the window in force and the one asked for next. Counted in `f64`.
- **The plugin's tail is counted from now**: the lap in force, plus the reverse hold in front.
- **The snap yields to self-oscillation**: above `sings_at` the loop is a generator.
- **A snapped core stops (`Idle`), it does not merely clear**: the chip's noise would refill a
  running line within one lap.

## Dependencies: none at runtime, and no framework types

- **`[dependencies]` is empty**, which earns the MSRV 1.87 override (`cargo tree -e normal,build`).
  `[dev-dependencies]` holds only **`mxm-measure`**; `C64` and `Rng` are written here
  ([NOTES.md](NOTES.md#dependencies-none-at-runtime-and-no-framework-types)).
- No `nice_plug::` anywhere; every public function takes plain values and a sample rate.

## Realtime rules

- No allocation in any per-sample path. Both lines and both reverse windows are allocated in
  `Core::new`, at the longest length the product reaches, so a `Line` change is an index change.
- `flush()` on every recursive state; `reset()`/`clear()` leave no tail.
- **No clear may cost the sample rate**: every clear runs inside one audio sample on both lines.
  The reverse window is invalidated by a per-half written count, never zeroed
  ([NOTES.md](NOTES.md#clears-that-cost-one-sample)).
- **A half's count is its latest recording's**: each write sets the count, never raises it, so a
  growing window never replays older audio ([NOTES.md](NOTES.md#the-reverse-windows-written-count)).
- Denormals are flushed in the DSP itself, not by a framework FTZ guard.

## Numeric contracts

- `f32` in the audio path; `f64` for the clock, the filters' coefficients and their state, where
  precision loss compounds.
- **A non-finite sample is a zero where it enters**, at every seam: `Core::process`,
  `Unit::process` (input and return), `Bbd::process` and both halves of the compander. Never at the
  output; `flush` stays denormal-only ([NOTES.md](NOTES.md#why-a-non-finite-sample-is-zeroed-where-it-enters)).
- Every saturator is **bounded exactly in `f32`** and **monotonic**.
- The clock is clamped to `[CLOCK_MIN_HZ, CLOCK_MAX_HZ]` inside the DSP, not only in the parameter
  layer, which a modulation sum can drive past.

# Work Guidance

- The device is `research:effects/bucket-brigade-delay.md`. Facts and numbers cross that boundary;
  page images and verbatim text do not.
- Prefer a clear implementation to a clever one. This is reference-quality open source.
- A constant nobody measured says so, beside its definition, with what would replace it.

# Verification

**The rulers are shared, the thresholds are not.** Measurements come from `mxm-measure` (dev-only,
not in the shipped graph); every bound and its headroom stays in the test that argues for it.

```bash
cargo test -p mxm-bucket-delay-dsp
cargo clippy -p mxm-bucket-delay-dsp --all-targets
cargo run -p mxm-bucket-delay-dsp --release --example feedback_spike   # the two thresholds, per line
```

Properties the tests must keep asserting, because each regresses silently:

- the delay law against Panasonic's own two reference circuits, and again through the running line
- each line's `delay_bounds`, which are that law at the clock's limits and ordered by stage count
- the six taps at the MN3011's proportions — the **spacing** exactly, the absolute positions within
  the filters' own group delay
- turning Time bends pitch, in the right direction
- the sample-and-hold's sinc, measured with the filters divided out analytically
- each line's distortion is its own catalogue figure, and the noise floors are ordered
- the bias trimmer is a U with its minimum at the centre, three to five times at the ends
- silence in gives **exactly** zero out, and a finite tail reaches exact zero
- the snap never clears a repeat still held: in the buckets after `Time` is shortened, on a line
  `Wobble` holds slow, or in the reverse window
- clearing the reverse window rewrites none of its cells, and no sample from before the clear
  returns, however the window grows afterwards
- a window that grows plays only what the window before it recorded — at the window's own seam
  against that rule, and through the line against a twin never given the older audio
- every structural change is silent under a live tail
- an oscillation started at the top dies when the control comes back down, and every line sings at
  the same point on the control
- a parked instance wakes when **any** control carries the loop gain over the threshold
- bounded and NaN-free across the parameter space, including the self-oscillating region
- one non-finite sample followed by audio renders exactly what a zero would have, at each seam
  named in *Numeric contracts*

# Child DOX Index

No child AGENTS.md files.
