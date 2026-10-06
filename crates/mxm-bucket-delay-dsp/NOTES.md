# NOTES.md — crates/mxm-bucket-delay-dsp

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples.
AGENTS.md is the contract; this file is the reference it links to.

## Nothing above this crate

**The first DSP crate in this collection that is not an instrument's.** Both sibling effects depend
on the synth whose circuit they came from; this one has nothing above it, which is a real
simplification: there is no golden-digest obligation against an instrument's render, and no
equivalence burden to invent an analogue of.

## Why the crate is named for the product

**Named for the product, not for the device.** `mxm-bbd-dsp` was proposed and rejected: justifying a
name by reuse — the JUNO chorus is a bucket brigade, a flanger would be — is the pre-generalisation
the root forbids by name. A shared bucket-brigade API is earned by a second honest implementation
demanding it, against two real call sites, not by predicting one.

## Which published model each part follows, and why the two disagree

Both are implemented **from the papers' equations**. No code from either author, and none from any
third-party implementation, is used or consulted — neither paper carries a licence grant, and the
one reference implementation that exists carries none either. The plan's §8 records that gate.

- **The line and its resampling: Holters & Parker, DAFx-18.** A fixed-length line at its own clock
  rate, with the conversion between the host rate and the clock rate done by the circuit's own input
  and output filters rather than by an interpolator. Three things fall out for free that a model
  built the other way has to add afterwards: the pitch bend on a time change, both aliasing
  mechanisms, and the output hold that accounts for most of the datasheets' unexplained insertion
  gain. **Raffel & Smith's interpolated line was rejected**: for an echo whose clock is the control,
  an interpolator sits exactly where the tone lives, in series with the reconstruction filter it
  would be modelling.
- **The components: Raffel & Smith, DAFx-10.** The compander's structure, the filter topology and
  the polynomial nonlinearity.

## The two-phase clock is where the delay law's factor of two lives

`STAGES_PER_CLOCK_PERIOD = 2`, and it is a named constant rather than a bare `/ 2` because the first
implementation here ticked one stage per period. Every delay came out at exactly twice the law, and
**every test that did not measure against the catalogue passed**. The line is indexed in clock
periods; a tap `s` stages along comes back `s / 2` periods later.

`BUFFER_SLOTS` is `MAX_TICKS + 1` for the same kind of reason: the write happens before the taps are
read, so a tap at the full length would wrap onto the slot just written and return the input
undelayed. That defect was invisible on three of the four lines.

## `Line::delay_bounds` is the chip's own reach, and the parameter layer needs it

`[N / 2f_max, N / 2f_min]` — the delay law against [`CLOCK_MIN_HZ`] and [`CLOCK_MAX_HZ`], so it is
derived rather than chosen. It exists because a question the plugin has to answer cannot be answered
without it: **which tempo subdivisions are reachable**. `Time` selects the subdivision when `Sync`
follows the host, and the 1024-stage part cannot hold a half note at 120 bpm at any clock the MN3101
generates — so a control offering one would be a delay that claims to be in time and is not.

## One number from the literature is not used

Raffel & Smith's `THD = 1.01^(N/1024) − 1` predicts about 4 % at 4096 stages where Panasonic's own
catalogue prints 1 % typical and 2.5 % maximum. **The catalogue's figures are used**
(`Line::thd_typical`). The law's *shape* — distortion compounding with stage count — is right and is
the useful part; its constant is not established, and adopting it would bake a fourfold error into
the product's most audible quantity.

**The catalogue's THD order is not monotonic and must not be "fixed".** The MN3011 is a low-noise
part at 0.4 % where the shorter MN3007 is 0.5 %. Longer is dirtier across the family as a whole;
the six-tap part is the exception the manufacturer built. `Line::signal_to_noise_db` *is* monotonic,
and that is the ordering the product's premise rests on.

## One declared correction to Raffel & Smith

Their clipping cases, `1 − a − b` and `−1 − a + b`, do not meet their own polynomial, which reaches
`1 − b` and `−1 + b` at `|x| = 1`. The gap is exactly `a` — the constant the polynomial adds to keep
its output averaging around zero — so the clipping cases read as having been written before that
constant was included. Taken literally the curve steps by 0.125 and is not monotonic, which this
collection's numeric contract forbids. **The clamp is taken at the polynomial's own endpoints**,
which is the reading that makes the paper's own sentence about a smooth transition true.

The device's own input limit is clamped **on the input**, at full scale, not on the drive: every
part specifies a `V_i(max)` and clips there whatever its distortion figure is. Letting the clamp
move with the drive leaves the line's output bound proportional to `1/g`, which six laps of a loop
later is how a model blows up.

## The filters' alignment is chosen, and the reason is recorded rather than hidden

The orders and the cutoff are the documents': third order in, third plus second out, `f_co = 2 kHz`
in both of Panasonic's reference echo circuits. **The pole placement is a Butterworth alignment,
chosen**, because the sources do not settle which capacitor sits at which node — worked through on
the standard equal-R third-order Sallen-Key, the natural assignment of the anti-alias set puts a
real pole at 16 Hz.

## The loop's takeoff and return nodes are read off the circuit

Panasonic's MN3005 echo circuit (catalogue p. 58) settles it: the `Echo Control` pot sits across the
**output**, after reconstruction, and returns to the **input summing node**, ahead of the anti-alias
filter. So every lap re-applies the whole chain, which is why repeats degrade progressively — the
load-bearing behaviour of the product. Dirt applied once at the output is a clean delay wearing a
dirty coat.

Two departures, both declared: the compander's **position** is ours (Panasonic's circuits have none
at all), and `Return = Tail` gives the takeoff **its own reconstruction filter and expander state**,
because one post-mix chain cannot carry the six-tap mix to the output and tap 6 to the loop at once.

## The faders set shape; Mix sets the balance; Feedback sets regeneration

**One control, one meaning — and it took two measured corrections to get there.** The owner's
report was *"I cannot figure out how the Character / Feedback works"*, and they were right: opening
the six-tap constellation moved the singing point from nine tenths of the travel to **one sixth**,
because the tap faders were in everything at once.

- **The mixer normalises by `√Σg²`**, the root sum of squares, so the faders are *relative weights*
  and the output level does not move when one opens. Summing multiplied the level by the number of
  open faders; plain averaging divides by it, which is right for correlated taps and wrong for
  these — the MN3011's spacings are deliberately non-multiple, so its returns are decorrelated and
  average to `1/√6`. Measured at 0.42 against a single tap's 1.03 before this was corrected.
- **The loop divides by [`effective_taps`], the participation ratio `(Σg)²/Σg²`.** Even at equal
  level, six decorrelated returns regenerate faster than one: with the mixer fixed the singing point
  still slid 91 % → 67 % → 38 % as taps opened. A literal count would step as a fader crossed zero,
  and a step here is a jump in how hard the delay regenerates.

The result, `examples/feedback_travel.rs`, on all four lines and six tap settings: **88–93 %**.

**Two warnings for whoever changes the loop next.** Re-fit the law by measurement rather than
reasoning about it — it has been re-fitted twice and both times the obvious arithmetic was wrong.
And keep the measuring harness free of compensation of its own: `feedback_travel.rs` twice carried a
divisor left over from a previous fit, cancelled or doubled what the DSP was doing, and reported
confidently and wrongly both times.

## `mix` is a crossfade, and the fade rides the wet alone

`out = (1 - mix) * dry + mix * fade * wet`. The owner's ruling of 2026-09-06 replaced an added level
with a balance, so that the unit can be **the source alone at one end and the echo alone at the
other** — an added level cannot reach the wet by itself at any setting, which is what a send, a wet
print and setting the tone by ear all need.

- **`mix = 0` is bit-exact dry.** The dry gain is exactly one there, which is what
  [`Fade::Parked`] and *doing nothing costs nothing* rest on.
- **The structural fade multiplies the wet term only.** Dipping the dry through a `Line` or
  `Routing` change would make the source itself duck, which is a defect and not a wart.
- Tests state their claims at the mix they need: the dry is only visible below one, and the wet is
  only clean of the dry at one.

## Feedback is calibrated, not derived, and the running threshold is the one to use

`Line::sings_at` and `Line::starts_at` are **measured** by `examples/feedback_spike.rs`. They cannot
be derived: the compander is inside the loop, so its compressor lifts a decaying tail on every lap
and the line holds at a lower gain than the takeoff sum alone would say.

**There are two thresholds and calibrating to the wrong one is the trap `mxm-folded-spring` cost
three attempts on.** A line that has snapped to zero needs a quarter to a half more gain to start
than a running one needs to hold; a delay somebody is playing through is never silent, so the
control is mapped against the **running** figure. `feedback_gain` scales the curve by each line's own
threshold, so all four sing at the same place on the knob.

Two measurement mistakes were made here before they were caught, both of the shape that file warns
about: the from-silence probe first asked *"is it still growing"* of a loop that had reached its
limit by second four, and read every working setting as a failure.

## The snap to exact zero, and the one thing it must yield to

A quiet loop is cleared to exact zero once the quiet outlasts **the longest journey anything still
held can take, plus `SNAP_HOLD_S`** — the journey matters, because a delay can be silent at its
output while holding audio that has not come back yet, and snapping on the output alone truncates
the first repeat after a gap.

- **The journey is judged over the quiet, never at the controls now.** It is the longest lap either
  running line has *actually* run at since the last loud sample — the clock in force, `Wobble`
  included — and, in front of it while `Reverse` is in, the longest the window could hold a sample
  (twice the larger of the window in force and the one asked for next). The buckets clock out at
  whatever rate they run: judged against the current `Time`, a click at 2 s was cleared from inside
  the line when Time went to 0.3 s three quarters of a lap later (it is due at 1.575 s); a slow
  `Wobble` stretched a 2 s lap to 2.10 s, past the hold; and at 250 ms a reversed click, due at
  0.75 s, was cleared from the window before it reached the line. The quiet is counted in `f64`.
- **The plugin's tail is counted from now**, so it takes the lap in force — what is left of a
  journey through the buckets is never longer — and adds the reverse hold in front, because a click
  still in the window has not reached the line.
- **The snap yields to self-oscillation.** Above `sings_at` the loop is a generator and the hardware
  starts one from its own noise floor; clearing the line there would make that impossible.
- **A snapped core stops, it does not merely clear.** The chip's own noise never stops, so a core
  that cleared and kept running refills the line within one lap and never reaches exact zero. `Idle`
  is what makes the silence exact, and it is the same fact as the collection's rule that an effect
  doing nothing uses no CPU.

## Dependencies: none at runtime, and no framework types

**No *runtime* dependencies, and that is what earns the MSRV 1.87 override.** It is a claim about the
shipped graph, checkable with `cargo tree -e normal,build`, and `[dependencies]` is empty.

`[dev-dependencies]` holds **`mxm-measure`**, the collection's measurement rulers — zero dependencies
at this same floor, reaching only tests and `examples/`, never a shipped `.clap`.
[`../mxm-measure/AGENTS.md`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-measure/AGENTS.md)'s verification section checks that rather than
asserting it.

`C64` and `Rng` are written here for the same reason and are the precedent. No `nice_plug::`
anywhere; every public function takes plain values and a sample rate.

## Clears that cost one sample

- **No clear may cost the sample rate.** Every clear — a structural fade reaching zero, parking,
  the idle snap — runs inside one audio sample on both lines. The reverse window (a second per
  half) is invalidated by a per-half written count and never zeroed; an unwritten cell reads zero.
  The line's own buffer is a fixed `BUFFER_SLOTS` whatever the rate, and is the one fill left.

## The reverse window's written count

- **A half's count is its latest recording's, not everything since the last clear.** A half is
  rewritten only as far as the window in force reaches, so after a long `Time` and then a short one
  the cells past the short window still hold the long window's audio; counted since the clear, a
  window that grew again replayed it — 43,200 samples on 1 s → 100 ms → 1 s, as old as Time had
  sat short. Each write sets the count rather than raising it, so a growth reads silence for the
  span it exposes, and nothing changes where no stale cell was read.

## Why a non-finite sample is zeroed where it enters

- **A non-finite sample is a zero where it enters**, at every seam a sample arrives through:
  `Core::process`, `Unit::process` (input and return), `Bbd::process` and both halves of the
  compander. Never at the output: a NaN in a filter's state or a detector is re-read every lap and
  is never cleared while audio keeps arriving, and `f32::max` ignores one, so a detector holding it
  sits at its floor, finite and wrong. `flush` stays denormal-only, so a NaN made *inside* the model
  still fails the finiteness tests instead of turning into a silently dead line.

## The measurement rulers

**The rulers are shared, the thresholds are not.** `mxm-measure` is a `[dev-dependencies]` entry —
zero dependencies at this same 1.87 floor, and **not in the shipped graph**, which is what the
manifest's *no runtime dependencies* comment means. Measurements come from there; every bound and
its headroom stays in the test that argues for it.
