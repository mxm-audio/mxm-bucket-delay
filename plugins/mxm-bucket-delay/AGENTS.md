# AGENTS.md — plugins/mxm-bucket-delay

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

The CLAP plugin: twenty-one parameters, the two layouts, the activity contract, fifty factory presets and
the editor. The device and every measured constant are
[`crates/mxm-bucket-delay-dsp`](../../crates/mxm-bucket-delay-dsp/AGENTS.md); the UI shape is
[`docs/briefs/mxm-bucket-delay.md`](../../docs/briefs/mxm-bucket-delay.md), which is the gating
document.

This effect copies the **MN30xx component family**, not an instrument’s built-in circuit, so it owns
its DSP crate and no instrument render constrains it.

The history, measurements and reasoning behind each contract are in [NOTES.md](NOTES.md).

# Ownership

Owns `src/` (`lib.rs`, `params.rs`, `preset.rs`, `telemetry.rs`, `editor/`), `presets/`,
`examples/` (`build_presets.rs`, `cpu_spike.rs`, `bucket_delay_preset_audit.rs`), `tests/`, `control-map.json`,
`Cargo.toml`, `LICENSE` and `README.md`.

# Local Contracts

## Permanent identifiers

- `CLAP_ID` is `dk.mxm.mxm-bucket-delay`, assembled from `plugin_name!`, **never** from
  `CARGO_PKG_NAME` ([NOTES.md](NOTES.md#permanent-identifiers)). **The name stands.**
- Parameter ids are permanent. The `Line` positions are **stage counts** (1024/3328/4096/8192),
  never another maker's part numbers.

## Twenty-one controls, one control-map page

Every parameter does something; all stay on one page (the owner, 2026-09-05). **The map is for
performance controls** ([NOTES.md](NOTES.md#twenty-one-controls-and-the-split-that-makes-one-control-map-page-enough)):

| | |
|---|---|
| `fx.delay` | filled by **Mix** — the same quantity, already on the Effects page. No role minted |
| Seven new roles, one new page | Time, Feedback, Line, Spread, Bias, Wobble, Rate |
| The eighth slot | left free, deliberately |
| Unmapped | the six tap faders, Return, Filter, Routing, Reverse, Sync, Change, Rate sync |

## Time, Sync and Change

- **`TIME_GLIDE_S` is the glide**: Time *is* clock rate. `Change = Glide` uses the same ramp synced,
  `Change = Snap` assigns directly (`glide_glides_and_snap_snaps`)
  ([NOTES.md](NOTES.md#time-is-the-clock-so-its-smoother-is-the-glide)).
- **Change applies only while `Sync` is on** (`change_is_inert_while_time_is_free`), and is greyed
  while Time is free. A host that reports no tempo falls back to the Time knob.
- **`Time` is the delay control either way** (seconds free, the subdivision synced); no `Division`.
  Sync is the collection's one tempo sync: `params::TIME_SYNC` beside Time, `params::RATE_SYNC`
  beside the wobble's Rate, resolved once a block in `synced_rate`
  ([NOTES.md](NOTES.md#time-is-the-delay-control-whether-or-not-sync-is-on)).
- **Old projects migrate** the three-way `sync` in `Plugin::filter_state` (`migrate_sync`).
- **By knob position, not nearest seconds; clamped to the reachable window, not rescaled**
  (`params::time_bounds`). The law lives on `MxmBucketDelayParams`; never two copies.
- **The tempo reaches the editor through `Telemetry`** (an `mxm_tempo::TempoCell`), since a tempo
  reaches a plugin only inside `process`.

## The activity contract

- **Mix at zero is Off**: the wet fades out, the lines are emptied rather than frozen, and the core
  is not run. **A tail that has ended parks the same way** ([NOTES.md](NOTES.md#the-activity-contract)).
- **A non-finite input sample is not activity**: the input pass zeroes it with the subnormals.
- `ProcessStatus::Tail(n)` is **recomputed every block**, never latched, counted from now: the lap
  in force plus the reverse window's hold while `Reverse` is in.
- **Self-oscillation is `Normal`, not a large `n`**: a host may truncate a tail, never a generator.
- **Never skip a block while the loop gain is in the self-starting region**, so a parameter alone
  can wake a parked instance.
- `loop_gain_of` lives in the DSP and is called from both sides; never a second copy.

## One control, one meaning: shape, balance, regeneration

| | |
|---|---|
| **The six faders** | the constellation's *shape* — relative weights. The mixer normalises by `√Σg²`, so opening a tap redistributes the wash rather than adding to it |
| **Mix** | how much of it reaches the output, against the dry it is crossfaded with |
| **Feedback** | how much of it goes round again — and it sings at 90 % of the travel whatever the faders are doing |

`loop_gain_now` is `Feedback` alone: the faders and `Return` never move it
([NOTES.md](NOTES.md#one-control-one-meaning-shape-balance-regeneration)).

## Mix is a crossfade

`out = (1 - mix) * dry + mix * fade * wet`, so it reaches dry-only and wet-only
([NOTES.md](NOTES.md#mix-is-a-crossfade)):

- **Zero is still Off, and still bit-exact** — at `mix = 0` the dry gain is exactly one.
- **The wet's fade is on the wet term alone.** A structural change must not dip the dry.

## Two layouts, and nothing is ever summed

One in → two out and two in → two out, as `mxm-chorus-06` declares. Never sum two sources: it
silences anti-correlated material ([NOTES.md](NOTES.md#two-layouts-and-nothing-is-ever-summed)).

## Presets

- **Init opens at `Mix` = 50 %** (`params::DEFAULT_MIX`, which layout tests read) as a **plain
  single echo**; `Reverberation` opens the constellation, weighted by `params::LADDER`. **The ladder
  is a preset, not a hidden gain.** With the faders exposed, the faders *are* the ladder
  ([NOTES.md](NOTES.md#the-presets-are-where-the-constellation-lives)).
- `examples/build_presets.rs` **is** the fifty-preset bank; `presets/*.json` is generated output, and
  a hand edit there is discarded by the next run
  ([NOTES.md](NOTES.md#fifty-presets-and-the-table-is-the-source)).
- **The table uses the panel's words** and each parameter's own parser; a refused string panics; an
  unmentioned control keeps Init's value. A synced patch writes `("sync", "On")` and its Time as
  `at("1/4")`, a knob position.
- `every_factory_preset_lands_where_its_description_says` unnormalises each one.
- **Presets are chosen against `examples/bucket_delay_preset_audit.rs`**, before and after any
  change; set levels from the rich held note and keep all three stimuli's reports visible
  ([NOTES.md](NOTES.md#the-presets-are-measured-not-reasoned-about)).
- `tests/the_factory_bank.rs`: **no preset may clip** at −2 dBFS, **none may be inaudible**.

## The editor

- **Line, Taps and Character are stable keys 0–2, all Effects**, on the shared paging renderer,
  each as wide as its controls; one bar-less page when they fit. The window opens at
  `editor::REFERENCE` (`the_opening_size_is_the_budget_hugged`) ([NOTES.md](NOTES.md#dynamic-paging)).
- **Every card is a `mxm_ui::tree`**, described once in `sections::card`, drawn by `sections::paint`.
  **Floors are computed**, with no usability minimum. A size never comes from telemetry; `take_ring`
  and the tempo are read once, before the frame (`every_card_passes_the_tree_checks_in_every_state`).
- **The constellation's geometry comes from the DSP's own tap table**, its brightness from
  `Telemetry::take_ring`; it is **drawn at the delay in force, not at the knob**. A synced knob reads
  its division, and with no tempo its free value
  ([NOTES.md](NOTES.md#the-editor-is-three-cards-and-the-constellation-is-the-display)).
- **The paging renderer gives every card a top-down body.** Do not wrap `ModuleCard` bodies in a
  hand-rolled horizontal layout; that places every control on one unbounded line.
- **The panel's height is measured from the paging report's visible card rectangles**, not only
  from `ui.min_rect()`: an outer allocation can under-report children and hide clipping.

## Parameter text round-trips

- **Parameter text is idempotent through the host's normalized conversion**: formatted, parsed,
  normalized and formatted again, it is the same string
  ([NOTES.md](NOTES.md#every-value-this-plugin-writes-down-it-reads-back-to-the-same-text)).
- **A formatter that changes unit or precision chooses the branch from the rounding its finer branch
  prints**, never from the raw value (`v2s_ms`, `v2s_hz`).
- **A parser strips the unit its formatter wrote.**
- `every_parameter_text_is_idempotent_through_the_hosts_conversion` holds it at every branch point.

# Work Guidance

- The brief is the gating document for the editor; implement it, do not re-decide it.
- `src/editor/binding.rs` re-exports `mxm_preset::binding`, the collection's one binding (2026-09-24); it was
  the seventh verbatim copy.

# Verification

```bash
cargo test -p mxm-bucket-delay
cargo clippy -p mxm-bucket-delay --all-targets
# Every page, light and dark, for review -> target/layout-tree/mxm-bucket-delay/<MXM_PICTURES tag>/
MXM_PICTURES=after cargo test -p mxm-bucket-delay --lib tree_pictures -- --ignored
cargo xtask bundle mxm-bucket-delay && clap-validator validate target/bundled/mxm-bucket-delay.clap
cargo xtask bundle mxm-bucket-delay --release && clap-validator validate target/bundled/mxm-bucket-delay.clap
cargo run -p mxm-bucket-delay --release --example build_presets  # the table -> presets/*.json
cargo run -p mxm-bucket-delay --release --example bucket_delay_preset_audit   # before and after any preset change
cargo run -p mxm-bucket-delay --release --example cpu_spike      # a block's cost against its budget
```

The player audition is a **test**, not a session, and it runs here:

```bash
cargo test -p mxm-bucket-delay-host-tests --test effect_chain --release
```

It runs `mxm-mono-01` into this effect through the player's own worker: repeats arrive a delay time
later, **off returns the dry to the bit**, the graph reaches exact silence, and an effect switched
back on after a gap starts from silence ([NOTES.md](NOTES.md#the-player-audition)).

Properties the tests must keep asserting, because each has already regressed once:

- **the panel paints every card and every control**, by its own painted label — a fit test cannot see
  an editor whose contents ran off the right of the window, and a person had to report that one
- **every parameter is bound exactly once**, or it falls silently out of every preset and out of Init
- **`Time` moves the delay whether or not `Sync` is on**, walks the subdivisions in order, and stops
  where the chip does
- **a gesture on every control**, through the shipped panel, in both themes — begins and ends balanced
- **every parameter's text survives the host's round trip** at its branch points, not only on the
  validator's grid
- nothing is painted outside the reference window, and no label wraps into a column of letters
  (`MXM_DUMP=1` prints where each label landed)

**Still to run:** the design system's §15 QA gate by eye, and Bitwig — the collection plan's
real-DAW gate, which neither sibling has passed either and which this product does not get an
exemption from.

# Child DOX Index

No child AGENTS.md files.
