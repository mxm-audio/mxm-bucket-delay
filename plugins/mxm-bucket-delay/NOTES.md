# NOTES.md — plugins/mxm-bucket-delay

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples.
AGENTS.md is the contract; this file is the reference it links to.

## Permanent identifiers

- `CLAP_ID` is `dk.mxm.mxm-bucket-delay`, assembled from `plugin_name!` and **never** from
  `CARGO_PKG_NAME`: a directory rename would otherwise change the plugin's permanent identity with
  no compile error and orphan every preset written under the old one.
- **The name stands.** A display-name resemblance does not collide with a `dk.mxm.*` reverse-DNS ID
  under the project’s domain; hosts key on the permanent ID.
- Parameter ids are permanent. The `Line` positions are **stage counts** (1024/3328/4096/8192), not
  part numbers: a parameter label may not carry another maker's model designation, and the stage
  count says the thing that matters anyway.

## Twenty-one controls, and the split that makes one control-map page enough

The owner's ruling, 2026-09-05: *no goal to have few parameters as long as they all do something*,
and *keep all controls on one page if at all possible*. Both hold at once because **the map is for
performance controls**.

The taps are the product's *preset* dimension; the mapped seven are its *performance* dimension.
That split is the argument, and it is why the Effects page being full did not force a second page.

## Time is the clock, so its smoother is the glide

`TIME_GLIDE_S` is not zipper-noise removal. Time *is* clock rate, so a moving target drags the pitch
of everything already in the line — the product's defining behaviour. The same ramp serves a synced
Time with `Change = Glide`; `Change = Snap` assigns the new time directly, which is in time and
audibly digital, and the two are separated **by measurement** in `glide_glides_and_snap_snaps`
rather than by which sounds nicer. **Change applies only while `Sync` is on**
(`change_is_inert_while_time_is_free`): a free knob glides whatever Change says, so a free delay is
the build before the split to the bit, and the editor greys Change while Time is free.

A host that reports no tempo falls back to the Time knob. A delay that fell silent without a
transport would be broken rather than honest.

## `Time` is the delay control whether or not `Sync` is on

`Time` selects seconds free and the subdivision synced. There is no separate `Division` parameter:
one control owns one time quantity either way.

**Sync is the collection's one tempo-sync interface** (`plans/plan-tempo-sync-controls.md`,
`plugins/AGENTS.md`): the quarter note beside Time, on the shared ladder `params::TIME_SYNC` (1/32 to
a half note, `mxm-tempo`), which is exactly the eleven-step table this plugin had, so every stored
Time position keeps its subdivision. The wobble's Rate has the same pair — `ratesync` beside it on
`params::RATE_SYNC`, every LFO's ladder, the top the fastest — resolved once a block in
`synced_rate` and applied over the free smoother, which keeps advancing.

**The old three-way `sync` was split on 2026-09-25** into `sync` (a Bool, the same id) and `change`
(Glide/Snap, the old variant ids), because the owner's rule is that glide or snap is not a sync
state. `Plugin::filter_state` reads an old project's `"free"`/`"glide"`/`"snap"` into Off/On/On with
the matching Change (`migrate_sync`, `an_old_three_way_sync_migrates_into_sync_and_change`). An old
user preset *file* stored Glide at 0.5, which a Bool reads as Off; the plugin is pre-release and
that was accepted.

- **By knob position, not by nearest seconds.** `TIME_SYNC` reads where the knob sits on its
  own range and takes that subdivision. Nearest-in-seconds would let a patch stored as 1/8 at 120
  bpm come back as 1/16. at 90, which is the one thing a synced delay must not do.
- **The reachable set is the tempo's and the chip's**, which is exactly what the owner asked for:
  `Line::delay_bounds` is the delay law against the clock the MN3101 generates, so the 1024-stage
  part cannot hold a half note at 120 bpm however the knob is turned.
- **Clamped to that window, not rescaled onto it**, so a knob position means the same subdivision at
  every tempo it is reachable at; the travel goes flat at an end instead, which is what the knob
  already does against the clock's own limits free. `params::time_bounds` is that window.
- **The law lives on `MxmBucketDelayParams`**, not in the plugin, because the editor needs the same
  answer — the same rule as `loop_gain_of`, and for the same reason: two copies would be free to
  disagree silently.

## The activity contract

- **Mix at zero is Off**: the wet fades out, the lines are emptied rather than frozen, and the
  core is not run. A frozen line spills a stale repeat from audio minutes old.
- **A tail that has ended parks the same way**, and the DSP stops rather than merely clearing —
  see the DSP contract's snap section for why that is the only way the silence is exact.
- **A non-finite input sample is not activity.** The input pass zeroes it with the subnormals, so
  it neither reaches the DSP nor ends a tail's status; the DSP rejects one at its own seams as well.
- `ProcessStatus::Tail(n)` is **recomputed every block**, never latched, which is what makes it
  follow `Feedback`, `Line`, `Return`, a tap fader, `Time` and `Sync` without naming any of them.
  It counts from now: the lap in force, and in front of it the reverse window's hold while
  `Reverse` is in, since a click waiting there has not reached the line. The DSP's snap is judged
  over the quiet instead — see its contract.
- **Self-oscillation is `Normal`, not a large `n`.** A tail may be truncated by a host; a generator
  must not be.
- **The plugin never skips a block while the loop gain is in the self-starting region**, so a
  parked, silent instance can be woken by a parameter alone. MXM Player's FX chain
  (`apps/mxm-player`, in mxm-player) had this defect and fixed it; the product does not rely on a
  host being more generous than the player was.

`loop_gain_of` lives in the DSP and is called from both sides. Two copies of that formula would be
free to disagree about when the line sings.

## One control, one meaning: shape, balance, regeneration

Shape, balance and regeneration are independent: the table in AGENTS.md states each.

The DSP's contract carries the two measured corrections that make that true, and the warning to
re-fit them by measurement rather than by arithmetic. `loop_gain_now` is therefore `Feedback` alone:
the faders and `Return` no longer move it, so the recomputed tail and the wake condition follow one
control instead of seven.

## Mix is a crossfade

Mix must reach both dry-only and wet-only output so the effect can be used as an insert, send or
printed wet track. An added-wet level cannot provide that range.

`out = (1 - mix) * dry + mix * fade * wet`. Consequences that are contracts:

- **The six shipped presets were rescaled by `v / (1 + v)`**, which is the mix that holds each
  patch's own wet-to-dry ratio where an added level had put it. They therefore sit around a third
  rather than around a half, and every one of them is about 3 dB quieter than it was.

## Two layouts, and nothing is ever summed

One in → two out (one source driving both lines, its dry copied to both outputs) and two in → two
out (each channel keeps its own source and its own dry). The pair `mxm-chorus-06` declares, and for
the same reason: both have a stereo output, because two lines with their modulation in antiphase is
what the stereo modes are. Summing two sources would silence anti-correlated material, which is a
defect rather than a wart.

## The presets are where the constellation lives

**Init opens at `Mix` = 50 %.** The center of a crossfade commits to neither end and demonstrates
both. Editor layout tests read [`params::DEFAULT_MIX`] rather than duplicating it.

The init patch is deliberately a **plain single echo**: one chip, one tap, engaged and trimmed. The
six-tap constellation is the most interesting thing the device family can do and nobody will arrive
at it by moving six faders, so `Reverberation` opens it — weighted by the reference circuit's own
100–150 kΩ resistor ladder (`params::LADDER`), which is a 3.5 dB tilt and not a decay envelope.

Preset files store *normalised* values, and this plugin has a skewed Time range and six enums —
`every_factory_preset_lands_where_its_description_says` unnormalises each one back through the
parameter, because a value a hundred milliseconds out looks exactly like a value that is right.

## Fifty presets, and the table is the source

**Why generated.** Fifty hand-typed files storing *normalised* values against a skewed Time range
and six enums is fifty chances to write a number that looks right and is a hundred milliseconds
wrong — the defect `every_factory_preset_lands_where_its_description_says` exists to catch. The table
instead says what each patch *is*, in the words the panel uses, and the plugin's own parsers turn
that into numbers:

```text
("time", "420 ms")  ->  the Time parameter's own string_to_value
("line", "8192")    ->  the enum's own variant name
("time", at("1/8.")) -> the knob *position* that selects a subdivision (TIME_SYNC.position)
```

A string a parameter refuses is a panic naming the id and the text, so a typo cannot become a
preset. What a patch does not mention keeps **Init**'s value, so every entry reads as a difference
from the plain echo rather than as twenty-one numbers. A synced patch writes `("sync", "On")`, and
`("change", "Snap")` where it snaps.

**Seven families**, and the split is the argument: single-tap echoes, synced echoes, six-tap washes,
sparse-tap rhythms, modulation, reverse, the warts (below-spec clock, hard bias, a line left singing
on its own noise) and routing. A bank that was fifty variations on one echo would be worse than six.

**`at("1/4")` is how a synced patch is written**, because synced the knob is a *selector*: a preset
has to store a position, and `1.07 s` is a number nobody could check by reading. Such a preset's
`text` is that position in seconds — what the knob would mean with `Sync` off — and that is the
value stored, not a mistake.

## The presets are measured, not reasoned about

**A preset is the one thing here no test could check**, because its only contract is that it sounds
like its name. So they are chosen against `examples/bucket_delay_preset_audit.rs`, which renders each one through
the shipped plugin and prints what it measures at — and the audit found a real defect no test
would have:

- **`Reverberation` was doing almost nothing.** Its `Feedback` was chosen when the loop gain was the
  control *times the takeoff sum*, so six open taps sang at 16 % of the travel and 25 % was a
  sensible-looking number. Under the re-fitted law that same 25 % is a loop gain of **0.08** — one
  pass through the line and a 0.7 s tail. The patch the whole product argues for was inaudible as a
  wash. It is 78 % now, a 4.9 s tail.
- **`Mix` was arithmetic, not a choice.** All six sat within 31–38 % because they were rescaled by
  `v / (1 + v)` from their old added levels. They are chosen per patch now: `Chorus` at 50 %, which
  is what makes a comb rather than a light vibrato; `Reverse swell` at 60 %, because the reverse
  *is* the effect; `Long echo` at 35 %, because repeats belong under the source.

The audit uses three stimuli: a pluck for onset, a rich held note for program level, and a pure tone
for maximum-correlation stress. A short pluck cannot grade a long reverse window, and a one-second
note can hide a 300 ms wash. Pure-tone peaks are not program-level gain: the tap mixer normalizes
`√Σg²` for decorrelated returns, while correlated taps sum. Set levels from the rich held note and
keep all three reports visible.

`tests/the_factory_bank.rs` is the floor under the audit, and it is a *rendering* test rather than a
reading one: **no preset may clip** on a note at −2 dBFS, and **no preset may be inaudible** against
the dry it replaces. Both are things every value in a file can be exactly what its author intended
and still be wrong about.

## Dynamic paging

Line, Taps and Character are stable keys 0–2, all Effects, in that order. They use the shared
paging renderer, each as wide as its controls (its ceiling is its floor), with no preferred
cross-card group. They merge to one bar-less page when they fit and split when they do not. The
window opens at the quarter-4K budget hugged to the three on one row (`editor::REFERENCE`, held by
`the_opening_size_is_the_budget_hugged`). Controller pages, parameter IDs, presets
and audio are unchanged. Existing painted-label/display/fit tests remain; native/DPI and DAW gates
are separate.

**Every card is a `mxm_ui::tree`** (mxm-kit's [`crates/ui/NOTES.md`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/ui/NOTES.md#a-card-body-as-data--tree), *A card body as data*).
`sections::card` describes each body once — the six taps, Feedback and Mix, and Bias, Wobble and
Rate as the collection's knob rows (`mxm_ui::tree::knob_row`), Time and Spread at the collection's
knob column (`mxm_ui::control::knob_column`) beside what stands with them, the switches
with Filter and Reverse sharing one cell, Return beside
Spread's circle, each quarter note beside its knob (`tree::switch_beside_knob`), Change greyed
(`tree::disabled`) while Time is free — and that description is measured for the card's floor
and height and drawn leaf by leaf through the bindings (`sections::paint`), through
`paging::editor::show`. **Floors are computed**, each tree's narrowest plus the card's
chrome, with no usability minimum declared beside them. The constellation states its own size:
`CONSTELLATION_HEIGHT` tall and at least `CONSTELLATION_MIN_WIDTH` wide, filling its card. A synced
knob's column holds its free readings and every division (`binding::synced_widest`), so no tree
takes the tempo. A size never comes from telemetry. `take_ring` and the tempo are read
once, before the frame. `every_card_passes_the_tree_checks_in_every_state` runs the shared checks
(`mxm_plugin_test::tree_checks`) at Init, every control at its top (ringing, loop from the last
tap) and at its bottom, and both syncs on with no tempo and with one.

## The editor is three cards, and the constellation is the display

Line, Taps, Character — left to right, following the signal, with `Sync` beside `Time` because it
is what Time *means*, and `Rate sync` beside the wobble's Rate. A tap paints its number on the Taps card (*Tap 3* is what a host, a tooltip
and a screen reader read), and a switch's cells are its parameter's own option text. No view bar while all three fit; derived Effects pages otherwise.

The constellation places six marks at their **actual** delay on a scale that does not rescale
itself, so the uneven spacing is visible, Time stretches the set together, and Spread opens it about
its last mark. Its geometry comes from the DSP's own tap table, so a display and a sound that
disagreed would be a compile error. Its brightness is `Telemetry::take_ring` — the wet the audio
thread actually added — so a line that has snapped to silence goes dark.

**A synced knob reads its division** (`1/8`), and with no tempo its free value, which is then the
value in force — the collection's one rule (`plans/plan-tempo-sync-controls.md`). It replaced a
read-out under the old three-way switch (`1/8 · 250 ms`, *Free*, *No tempo*), which said the same
thing in a line of its own.

**The constellation is drawn at the delay in force, not at the knob**, for the same reason: when
`Sync` follows the host the knob is a subdivision selector, and marks drawn at its raw value would
sit where the sound is not.

Three tests now stand where that got through, and none of them measures height:

- `the_panel_paints_every_card_and_every_control` reads egui's own shape list and asserts each
  control's label is painted. `MXM_DUMP=1` prints where each one landed, which is how the defect
  was found.
- `nothing_is_painted_outside_the_reference_window` is the other axis, the one the fit test cannot
  see.
- `no_painted_label_is_wrapped_into_a_column_of_letters` catches the second defect the same dump
  showed: two segmented switches sharing a row left `Reverse` about seven points for its name, and
  egui wrapped it one letter per line into a 7 x 98 box. Filter and Reverse are stacked now.

## Every value this plugin writes down, it reads back to the same text

**Parameter text is idempotent through the host's normalized conversion**: formatted, parsed,
normalized and formatted again, it is the same string. `clap-validator`'s `param-conversions` checks
that at random values, so a clean run proves nothing about a sliver.

- **A formatter that changes unit or precision chooses the branch from the rounding its finer branch
  prints**, never from the raw value. `Time` switched at a raw `v < 1.0` and `Rate` at `v < 1.0`, so
  0.9996 s read `1000 ms`, parsed to exactly one second and came back `1.00 s`, and 0.9996 Hz read
  `1.000 Hz` and came back `1.00 Hz` — or the mirror, whichever side the normalized inverse landed.
  `v2s_ms` decides on the rounded millisecond and `v2s_hz` on the rounded thousandth now.
- **A parser strips the unit its formatter wrote.** `param-conversions` once found `Rate` formatting
  `0.40 Hz` while its parser stripped only a lowercase `hz`.

`every_parameter_text_is_idempotent_through_the_hosts_conversion` holds it: every parameter in
`param_map`, with the unit on, at the validator's twenty-step grid, both sides of every branch point
its formatters have (`branch_points`), and a hair either side of zero on a range that crosses it. It
replaced a test that compared plain values within a tolerance with the unit off, which passed both
defects above.

## The player audition

It builds the real chain — `mxm-mono-01` into this effect — through the player's own worker and
asserts the four things §7 asks of it: the delay is reached and its repeats arrive a delay time
later, **off returns the dry to the bit**, the graph reaches exact silence, and an effect switched
back on after a gap starts from silence. `apps/mxm-player/AGENTS.md`'s *Auditioning a plugin here*
is the general form; since the split it is mxm-player's
[`apps/mxm-player/NOTES.md`](https://github.com/mxm-audio/mxm-player/blob/main/apps/mxm-player/NOTES.md#auditioning-a-plugin-here-with-no-window-and-no-sound-card).
