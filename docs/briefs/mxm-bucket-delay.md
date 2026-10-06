# mxm-bucket-delay — UI design brief

Required by `MXM_DESIGN_SYSTEM.md` §14. Answers the ten questions in order, then records the
deliberate deviations.

**Plugin:** a bucket brigade delay. Audio in, audio out; no notes. Product id
`dk.mxm.mxm-bucket-delay`, named for the device rather than a box, per
`plans/plan-mxm-fx-collection.md` §1.

**The reference is a device, not a box**: `research:effects/bucket-brigade-delay.md`, read from
Panasonic's own *BBD Bucket Brigade Devices* catalogue and the two DAFx papers that model the part.
Every number in this brief attributed to the hardware comes from there.

**This is not `mxm-mono-00`'s delay, and does not promote it.** That delay is a clean interpolated
line with fixed 0.45 feedback and one-pole damping at 4 kHz, modelling the SYSTEM-100 plug-out's
*software* delay — a chosen model over an unpublished algorithm
(`research:effects/system-100-plugout-delay.md`). It is not a bucket brigade and must not become
one: `plugins/AGENTS.md` forbids a promotion changing the instrument's render. `mxm-mono-00`'s
delay stays on the waiting list with its own reference box and its own identity still to settle.

---

## 1. Primary sound-design task

**Deciding how far into the past the repeats live, and how badly they have aged getting there.**

In a digital delay, time, brightness, noise and grit are four independent choices. In a bucket
brigade they are one, because they are all consequences of the clock: the only way to lengthen the
delay is to slow the sampler, and a slower sampler is darker, noisier, dirtier, and bends the pitch
of whatever is already in the line while you turn the knob.

**That coupling is the product.** Break it and you have a digital delay with a low-pass on it, which
is the thing everyone else builds. So the flexibility here comes from opening what the hardware
welded shut — the chip, the bias trimmer, the mixing resistors, the filter regime — and never from
decoupling time from tone.

## 2. The three to five parameters users reach for most

Four, then a bank:

1. **Time** — the clock. The delay of the last tap, and the only real control the hardware had.
2. **Feedback** — the loop. Panasonic's own echo circuit has exactly one pot and this is it, marked
   *Echo Control*. It sings in the last tenth of its travel and stops when it comes back down.
3. **Mix** — a crossfade: the instrument alone at one end, the echo alone at the other.
   **This brief argued the other way and was overruled on hearing it**, 2026-09-06. The argument
   was the canon's — a BBD pedal's third knob is a level, and both sibling effects add — and what
   it missed is *reach*: an added level can never present the wet by itself, so the delay cannot be
   sent to, cannot be printed as a wet track, and cannot have its tone set by ear. Zero is still the
   dry alone, to the bit, so *off is off* is untouched.
4. **Tap 1–6** — the mixer, below.

**The count is not three, and is not trying to be.** The owner's ruling, 2026-09-05: *there is no
goal to have few parameters, as long as they all do something.* This effect has **twenty-one** —
twenty until `Division` was folded into `Time`, then nineteen until the one tempo-sync interface
split `Sync` and gave Rate its own (2026-09-25) — and §5 argues each one. The two sibling effects
have three because a chorus and a spring are small; a six-tap delay with a feedback loop is not, and
pretending otherwise would cost the sound.

### The tap mixer, and why it is the centre of the instrument

**The MN3011 is a six-tap BBD that Panasonic sold as a reverb**, and its listed feature is that its
six taps are *"not in multiple proportion with each other so that a proper mixing of the six
differently delayed output signals generates a highly effective reverberation."*

| Tap | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---|---|---|---|---|---|
| Stages | 396 | 662 | 1194 | 1726 | 2790 | 3328 |
| Ratio to tap 1 | 1.000 | 1.672 | 3.015 | 4.359 | 7.045 | 8.404 |
| Default level (the reference circuit's ladder) | 0 dB | −0.83 | −1.58 | −2.28 | −2.92 | −3.52 |

Not one is a multiple of another. Evenly spaced taps give repeats; these give a *spread*, and it is
the cheapest route to a reverberant character in the whole family.

The hardware mixed them with a fixed resistor ladder — 100 k, 110 k, 120 k, 130 k, 140 k, 150 kΩ.
**Six faders are that ladder made live**, which is a fixed quantity opened rather than an invention,
and it is where the presets will come from: tap 6 alone is a plain echo, all six is a wash, taps 1
and 4 is a rhythm, and every one of those behaves differently once Feedback is up.

**One bank, feeding both the output and the loop**, with a `Return` switch for the single
distinction a second bank would buy (§5). Twelve faders for that one difference is not a trade this
panel should make.

## 3. Signal flow that must be visible

```
  in --+----------------------- dry x (1-Mix) -------------------------+--> out
       |                                                              |
       +->[reverse]->[compress]->[anti-alias]--+                       |
                                               v                       |
            +-------- the line: N stages, clocked --------+            |
            |   .1....2......3........4..........5.....6  |            |
            +---+-----+------+--------+----------+-----+--+            |
                |     |      |        |          |     |               |
                +-----+------+---+----+----------+-----+               |
                          tap mixer x6                                 |
                                |                                      |
                                +-->[recon]->[expand]--+-> x Mix ------+
                                                       |
                                                       +-> x Feedback -+
                                                                       |
       +---------------------------------------------------------------+
       |  returns to the input summing node, ahead of [compress]
       v  so every lap traverses the whole chain
```

Three facts the panel must carry:

- **The taps are unevenly spaced, and they all move together**, because they share one clock. Time
  stretches the whole constellation; it does not slide one tap.
- **Everything that dirties the signal is inside the loop** — the nonlinearity, the noise, the
  compander and the filters. Repeats must degrade *progressively*. Dirt applied to the output
  instead gives a clean delay wearing a dirty coat, and that is what sounds fake.
  **The loop is the outer one**, and this is read off Panasonic's own echo circuit rather than
  chosen: the `Echo Control` pot sits across the output after reconstruction, and returns through a
  buffer to the input summing node ahead of the anti-alias filter.
  `plans/plan-mxm-bucket-delay.md` §2 carries the evidence.
- **The dry is never processed**, only weighed: `out = (1 - mix) * dry + mix * wet`.

## 4. Which controls belong in Play view

**Not applicable — no `Play` view**, and no view bar, exactly as `mxm-chorus-06` and
`mxm-folded-spring`. They do not need a second view drawn from the same list.

Twenty-one parameters need *grouping*, which is what cards are for. Three, left to right, following the
signal:

| Card | Holds |
|---|---|
| **Line** | Line, Time, Filter, Reverse |
| **Taps** | the six faders, Spread, Return — and the constellation display (§8) |
| **Character** | Bias, Wobble, Rate, Feedback, Mix, Routing |

`Sync` sits in the **Line** card, beside Time: it is what Time means, not a separate facility, and
putting it anywhere else would separate a control from its own unit. It is the collection's quarter
note, and Change sits under it. Synced, the Time knob reads its subdivision. `Rate sync` sits beside
Rate on the Character card the same way.

## 5. Advanced controls and their disclosure

**No hidden zone.** Every parameter is on the panel; the cards are the disclosure. Each one, and
what it does that nothing else does:

| Parameter | Kind | What it does, and why it is not redundant |
|---|---|---|
| **Time** | knob | The clock. Sets delay, and with it bandwidth, noise and pitch-bend-on-turn. **In the synced positions the same knob selects the subdivision**, over the ones this tempo and this chip can reach |
| **Feedback** | knob | Loop gain. Self-oscillates in the last tenth, as the hardware does on its own noise |
| **Mix** | knob | The balance. Dry alone at zero, wet alone at one |
| **Tap 1–6** | six faders | The mixing ladder, opened. The shape of the space |
| **Line** | 4-way | 1024 / 3328 / 4096 / 8192 stages. Sets *length, noise floor, bandwidth and distortion together* — two lines at the same delay time sound different, which is the point |
| **Spread** | knob | Stretches the tap constellation about tap 6. 1.0 is Panasonic's ratios. A declared departure (§9) |
| **Return** | 2-way | What feeds the loop: `Mix` (the bank's sum — dense, builds into a wash) or `Tail` (the last tap only — a clean long repeat under a busy output). The one thing a second fader bank would buy |
| **Bias** | knob | The trimmer. Centre is the measured distortion minimum; both directions grit up, three to five times at the extremes |
| **Wobble** | knob | Clock modulation depth. Not contamination — modulating the clock is how this same chip family makes chorus and vibrato |
| **Rate** | knob | Wobble rate: tape flutter at one end, seasick at the other. **Rate sync** (the quarter note beside it) makes the same knob pick a division of the host's tempo, the top the fastest |
| **Filter** | 2-way | `Fixed` (Sallen-Key at the hardware's 2 kHz, chosen for the longest delay, so short delays stay dark) or `Tracking` (switched-capacitor, cutoff follows the clock, so short delays stay bright). Both are real designs |
| **Routing** | 3-way | Mono / Stereo / Ping-pong |
| **Reverse** | 2-way | Buffers and flips *into* the line (§9) |
| **Sync** | on/off | Off — the hardware, `Time` is the clock. On — Time follows the host's tempo, the collection's one tempo-sync interface |
| **Change** | 2-way | What a synced tail does at a new tempo or subdivision. `Glide` carries it into the new time continuously, because time *is* clock rate: the tape behaviour, and the default. `Snap` is immediate, in time and audibly digital. Inert while Sync is off. It was Sync's third position until the owner's one sync interface (2026-09-25) made glide or snap not a sync state |

**Consequence for the developer channel**: it arrives as MIDI CC and an effect has no note port, so
this plugin carries none of it — `plugins/AGENTS.md` states that for effects generally.

**Control map: one new page** — the owner's ruling, 2026-09-05, *keep all controls on one page if at
all possible*. It is possible because **not every parameter earns a role**. Mix reuses the
existing `fx.delay`, seven new roles (Time, Feedback, Line, Bias, Wobble, Rate, Spread) take one new
page with a slot to spare, and the six tap faders and the mode switches stay unmapped — they are the
shape you set and store, not the controls you reach for while it plays.
`plans/plan-mxm-bucket-delay.md` §6 carries the mapping.

## 6. Views

**Space-derived pages**, following design-system §3.2. Line, Taps and Character are three
indivisible Effects cards in that order, not one card. No preferred cross-card group. They merge
to one bar-less page while they fit; narrow/short windows may split them with full-name navigation.
No duplicate Parameters list or developer MIDI channel; controller mappings remain unchanged.

## 7. Identity accent

**The collection's, unchanged**, for the reason both sibling briefs give: §5.3's identity hues tell
a *rack of instruments* apart, an effect is told apart by its name in a chain, and a third accent
scheme would have nothing keeping it from colliding with the instruments'.

## 8. Live visualizations

**One: the constellation.** Six marks on a line, placed at their *actual* delay against a scale that
does not rescale itself, so:

- **the uneven spacing is visible** — the single fact that explains why the effect sounds like a
  space rather than an echo, and no number on a knob conveys it;
- **Time stretches the whole set together**, which is what teaches that one clock drives everything;
- **Spread is visible** as the constellation opening and closing about its last mark;
- **each mark's brightness is its fader**, so a preset's shape is readable at a glance.

The loop is drawn as a return path from whichever taps `Return` selects, brightening with Feedback,
so the difference between `Mix` and `Tail` is a picture rather than a manual.

Brightness comes from `Telemetry::take_ring` — **the wet the audio thread actually added** — so a
line that has snapped to silence goes dark, and a `Line` switched mid-tail visibly dims through the
change. The geometry comes from the DSP's own tap table, so a display and a sound that disagreed
would be a compile error rather than a drawing mistake.

A **level meter** in the app bar, as every other editor has. A crossfade cannot clip on the sum the
way an added wet could, but a self-oscillating loop can still run hot on its own, and this is the
only place that says so.

## 9. What is removed from the source hardware layout, and why

The source layout is a three-knob pedal, or — for the MN3011 — no panel at all, because it is a chip
on somebody's board.

**Nothing is removed.** Everything below is either a fixed quantity opened or a declared departure.

### Fixed quantities opened — the reason to want a plugin at all

| Was welded by | Becomes |
|---|---|
| The chip soldered in | **Line** |
| A trimmer under the lid | **Bias** |
| The mixing resistor ladder | **Tap 1–6** |
| A filter regime chosen once at design time | **Filter** |
| The application the chip was bought for | **Wobble**, **Rate** |

### Declared departures — argued, not silent

- **Every line is tapped six times.** Only the MN3011 has taps; the MN3005 and MN3007 have one
  output. Here all four lines carry the MN3011's *proportions* scaled to their own length. The
  alternative was a tap mixer that does nothing on three of four settings, which the owner's ruling
  in §2 rules out directly — a parameter that does nothing is worse than a departure written down.
- **Spread.** The hardware's ratios are fixed by where the taps were fabricated. Scaling them is
  ours. It defaults to Panasonic's set, and the display makes the departure visible rather than
  hidden.
- **Reverse**, and *where it sits*. Charge in a BBD flows one way — Panasonic say so in their first
  paragraph — so the line cannot be read backwards. This buffers and flips **into** the line. That
  is the honest placement and it also sounds better: the reversed signal collects the dirt on the
  way through instead of arriving clean at the end.
- **Ping-pong and stereo.** The hardware was mono. Stereo has an honest form — two lines with the
  wobble in antiphase, which is exactly what the JUNO chorus does with two BBDs
  (`research:effects/juno-chorus.md`) — and ping-pong has none; it is routing, it sits outside the
  line, and it costs nothing in character.
- **Tempo sync.** Never on the hardware. It carries a consequence worth deciding rather than
  discovering: because time *is* clock rate, a sync change glissandos the tail rather than jumping.
  **Ramp is the default** and the behaviour to keep; snapping is the digital-sounding option.
  **It ships** — the owner's ruling, 2026-09-05 — as the `Sync` switch in §5. The *subdivision* is
  the Time knob rather than a parameter of its own: a `Division` control shipped alongside a dead Time
  knob and came straight back as *"when time is set to snap it is hardcoded to 1/8"* (2026-09-06).
  The glide-or-snap choice was Sync's third position until 2026-09-25, when the collection's one
  sync interface made Sync an on/off and the choice `Change`.
- **Below-spec clock.** The datasheet's minimum is 10 kHz. The long line is allowed below it, where
  charge leaks between transfers and the repeats go droopy and hissy. A wart deliberately reachable,
  and the display says when the line is out of specification.

### Delay ranges, which follow from the line and are not a chosen number

| Line | In spec (10–100 kHz clock) | Below spec |
|---|---|---|
| 1024 | 5.1 – 51 ms | — |
| 3328 | 16.6 – 166 ms | — |
| 4096 | 20.5 – 205 ms | — |
| 8192 | 41 – 410 ms | to about 820 ms |

**The ceiling is the product.** Stretch it to seconds and the character is gone; long tails come
from Feedback, not from a longer line.

## 10. Minimum size and 200% scale

**Resizable**; the editor's `REFERENCE` and `MINIMUM` and their tests hold the sizes. The three
Effects cards use the shared paging renderer, retaining their floors and ceilings. The six faders stay within the
indivisible Taps card; a fitting multi-card page never scrolls. Existing painted-label, display
and panel-fit tests remain. Independent zoom is **75–200%**; an indivisible overflow scrolls.
Keep physical window size fixed for §15's DPI/zoom gate. Native-window, real-DAW and owner
inspection remain open.

## 11. The Off state

Mix at zero is Off, and the editor shows it:

- The constellation's marks **go dark** — drawn at the rail's colour, not the accent.
- A badge reading **Off** sits at the display's bottom-right, with *Mix is at zero, so the line is
  empty and costs no CPU* on hover.
- The Mix knob keeps its normal appearance. It is not disabled — it is the way out.

Two channels, never hue alone: the badge is text, the marks' change is a fill.

A second state worth showing the same way: **out of spec**, when Time has pushed the clock below
10 kHz. A badge, and the constellation's scale marked past its limit.

## Implementation notes that belong to the brief

- **The DSP starts in this plugin's own crate, not in an instrument's.** `plugins/AGENTS.md`
  extracts on the strength of the second consumer, and there is one consumer today.
- **The nonlinearity, noise, compander and filters are inside the feedback loop.** §3. This is the
  load-bearing decision and the easiest one to get wrong.
- **Feedback's calibration follows `mxm-folded-spring`**: measured per line, singing in the last
  tenth, and stopping when the control comes down. The spring's three rounds of that are written up
  in its own `AGENTS.md`; do not re-derive them.
- **One number is not settled.** Raffel & Smith's `THD = 1.01^(N/1024) − 1` predicts 4.06 % at 4096
  stages where Panasonic print 1 % typical and 2.5 % maximum. The research page has it in the
  unverified list. Take the catalogue's figures; the law's *shape* is right and its constant is not.

## Deliberate deviations from the design system

| § | Rule | Deviation | Why |
|---|---|---|---|
| §14.2 | Three to five parameters users reach for most | Twenty-one parameters; four named as the ones reached for most | §2 — the owner's ruling of 2026-09-05, and a six-tap delay is not a three-knob effect |
| §5.3 | Each instrument has an identity accent | The collection accent is kept | §7 |
| §14.5 | Advanced controls and disclosure | No advanced zone | §5 |

## Sign-off checklist

- [x] §14's ten questions answered.
- [x] Every parameter has a stated job that no other parameter does (§5).
- [x] Each departure from the hardware is declared and argued, not silent (§9).
- [x] The one visualization is fed by the audio thread's own value, and its geometry by the DSP's.
- [x] The Off state is carried by two channels, not hue.
- [ ] The size floor is pinned by a test — at implementation.
- [ ] Feedback's per-line self-oscillation points measured — at implementation.
