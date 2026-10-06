# mxm-bucket-delay

A bucket brigade delay, built from the **device** rather than from a pedal.

Not affiliated with, endorsed by, or associated with Panasonic or Matsushita. No third-party
trademark is used to name this product or any of its controls.

## What it is

A bucket brigade device is a line of capacitors that passes a sample along one stage at a time,
clocked from outside. It was how you got an echo before you could afford a digital one, and it
sounds the way it does because of a single fact:

**The clock is the only time control there is.** Delay is stages over twice the clock, so the only
way to lengthen the delay is to *slow the sampler* — and slowing the sampler also narrows the
bandwidth, raises the noise, deepens the distortion, and bends the pitch of everything already in
the line. In a digital delay those are four independent choices. Here they are one.

Every control in this plugin exists to keep that coupling intact.

## Why a family and not a box

Every other effect in this collection copies one machine. This one copies a *component*, because the
catalogue turned up something no famous pedal ever used: the **MN3011**, a six-tap device sold as a
reverb whose taps are deliberately *"not in multiple proportion with each other"*. Six returns at
uneven spacings do not sound like an echo — they sound like a space. Picking a pedal would have hidden
the most interesting thing the family can do behind that pedal's three knobs.

So the six taps are faders, and every line wears that constellation, scaled.

## The controls

**Line** — 1024 / 3328 / 4096 / 8192 stages. Four different chips: length, noise floor, bandwidth
and distortion move *together*, which is why two lines set to the same delay time do not sound the
same.

**Time** — the clock. Turn it while something is repeating and the tail bends, the way a tape
machine does. When `Sync` is following the host the same knob steps through the subdivisions, over
the ones the tempo and the fitted chip can actually reach — the short line cannot hold a half note
at 120 bpm at any clock, and the knob says so by going no further.

**Sync** — the quarter note beside Time. Off is the hardware; on, Time follows the host's tempo and
the knob reads its subdivision, `1/8`. **Change** is what a synced tail does at a new tempo: `Glide`
carries it into the new time continuously, `Snap` takes it at once, in time and audibly digital.

**Rate sync** — the quarter note beside the wobble's Rate: the same knob picks a division of the
host's tempo, the top the fastest.

**Tap 1–6, Spread** — the mixing ladder, opened. Spread stretches the constellation about the last
tap; at 1.00× it is the six-tap chip's own spacing.

**Return** — what feeds the loop: the whole tap mix, which builds into a wash, or the last tap alone,
which repeats cleanly underneath it.

**Feedback** — every repeat goes round the *whole* circuit again — filters, distortion, noise,
compander — so the repeats wear out rather than simply getting quieter. In the last tenth the line
sings on its own noise, as the hardware does.

**Bias** — the trimmer every unit left the factory set to, and every unit has drifted since. Centre
is the distortion minimum; both directions grit up, three to five times at the ends.

**Wobble, Rate** — clock modulation. Not contamination: modulating the clock is how this same chip
family makes chorus and vibrato.

**Filter** — `Fixed` is the hardware's own 2 kHz filter, chosen for the longest delay a real box
reaches, so short delays stay dark because the filter does not know how fast the clock is running.
`Tracking` follows the clock. Both are real designs.

**Routing** — mono, stereo with the two lines modulated in antiphase, or ping-pong.

**Reverse** — buffers the input and flips it into the line. The repeats reverse; the loop's own
return does not.

**Mix** — a crossfade, not an added level: at one end the instrument alone, at the other the echo
alone with no dry at all. **At zero the effect is off**, the lines are emptied, and it costs no CPU
at all.

## Presets

**Fifty of them, and Init.** The plugin opens as a plain single echo; the bank is where the rest of
the device lives, in seven families:

| | |
|---|---|
| **Echoes** | one tap, from `Slapback` to `Runaway` — and the synced ones, `Quarter note` through `Sixteenth stutter` |
| **Washes** | all six taps into the loop: `Reverberation`, `Chamber`, `Hall`, `Cavern` |
| **Rhythms** | a sparse constellation is a pattern, not a space: `Two tap`, `Gallop`, `Scatter` |
| **Modulation** | the clock moved, which is how this chip family makes `Chorus`, `Vibrato` and `Deep flange` |
| **Reverse** | `Reverse swell`, `Backwards wash` |
| **The warts** | reachable on purpose: `Below spec`, `Gritty`, `Starved`, `Singing line` |
| **Routing** | `Ping-pong`, `Wide ping-pong`, `Mono tight` |

`Reverberation` is the one to try first: it opens all six taps at the reference circuit's own
resistor weighting, which is what the six-tap part was sold to do. `Chorus` is the same device at a
very short delay with no feedback at all.

## Building

```bash
cargo xtask bundle mxm-bucket-delay --release   # -> target/bundled/mxm-bucket-delay.clap
```

## Licence

GPL-3.0-or-later — see the repository's [`LICENSE`](../../LICENSE) at its root. All code here is
original. The two published models it follows are cited in the source; no code from their
authors, and none from any third-party implementation, is used.
