# mxm-bucket-delay

An MXM effect: A bucket brigade delay, from the device rather than a box: six taps, four chips and a clock that is the only time control.

Part of the MXM collection: every instrument, effect and tool lives in its own repository under
[github.com/mxm-audio](https://github.com/mxm-audio), built on
[mxm-kit](https://github.com/mxm-audio/mxm-kit).

## Building

Rust 1.95 or newer. On Linux, install ALSA, JACK, X11, xkbcommon and a GL loader first.

```bash
cargo xtask bundle mxm-bucket-delay --release   # -> target/bundled/mxm-bucket-delay.clap
cargo test
```

Copy `target/bundled/mxm-bucket-delay.clap` into your CLAP folder. This is pre-alpha:
nothing is released, so there are no official builds yet.

## Licence

GPL-3.0-or-later — see [`LICENSE`](LICENSE) and [`NOTICE.md`](NOTICE.md). The MXM
name and logo are trademarks; [`TRADEMARKS.md`](TRADEMARKS.md) says how they may be used.

Contributions are welcome: see [`CONTRIBUTING.md`](CONTRIBUTING.md). How the repository is
organised, and the rules each part keeps, are in [`AGENTS.md`](AGENTS.md).
