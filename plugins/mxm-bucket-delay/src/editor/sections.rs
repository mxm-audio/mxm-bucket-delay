//! The three cards, the constellation, and the sentence each control's tooltip carries.
//!
//! [`all_parameters`] is also what `preset.rs` walks — capture, Init, resolve and the dirty
//! baseline all iterate it — so a parameter missing from it would silently fall out of every
//! preset, which is what `every_parameter_is_bound_exactly_once` guards.
//!
//! # Three cards, following the signal
//!
//! The brief's §4: **Line** (what the clock is doing), **Taps** (where the signal comes back from),
//! **Character** (what the circuit does to it on the way). Twenty-one parameters need grouping and
//! this is the grouping the signal itself suggests — `Sync` and `Change` sit beside `Time` because
//! they are what Time *means*, not a separate facility.
//!
//! # The row is `mxm_ui::flow`'s, and the first version of this file got that wrong
//!
//! The three cards are laid out by [`mxm_ui::flow::cards`], the collection's wrapping-rows layout,
//! and each body below draws into the `Ui` **it** provides. The first version put the cards in a
//! hand-rolled `ui.horizontal_top`, which reads as though it should work and does not: a card's
//! body inherits its parent's layout, so every control inside every card was laid out *across*
//! rather than down. The panel ran to 3284 points wide in a 1020-point window, and all a person saw
//! was the first card's title.
//!
//! It passed the fit test, which measures height and cannot see a row running off the right edge.
//! `the_panel_paints_every_card_and_every_control` is the test that catches it, and it reads egui's
//! own shape list rather than the layout's bookkeeping.

use std::collections::HashMap;

use egui::{Rect, Ui};
use mxm_bucket_delay_dsp::bbd::{MN3011_STAGES, MN3011_TAP_STAGES, TAPS};
use mxm_ui::control::Size;
use mxm_ui::space::{SPACE_2, SPACE_4};
use mxm_ui::theme::Tokens;
use mxm_ui::tree::{self, Height, Kind, Node, Share};
use nice_plug::prelude::ParamSetter;

use super::binding::{Bound, segmented};
use crate::params::MxmBucketDelayParams;
use crate::telemetry::Telemetry;

/// The knobs' size. Time and Feedback are what a player performs with, so they take the largest the
/// system has; the rest of the character controls are one step down, which is what keeps a row of
/// six on one line.
const PRIMARY: Size = Size::Primary;
const SECONDARY: Size = Size::Standard;

/// The constellation display's height, and the floor under it.
const CONSTELLATION_HEIGHT: f32 = 96.0;
/// The narrowest the constellation may be before a tap's position stops reading as a position.
pub const CONSTELLATION_MIN_WIDTH: f32 = 220.0;
/// A tap mark's width, and the loop path's thickness.
const MARK_WIDTH: f32 = 3.0;
const LOOP_THICKNESS: f32 = 1.5;
/// The size of the display's own small text.
const CAPTION_FONT: f32 = 10.0;
/// How far the caption sits in from the box's corners.
const CAPTION_INSET: f32 = 9.0;

/// Every parameter, in declaration order, with the sentence its tooltip carries.
///
/// The descriptions are here because **only the plugin has them**: CLAP carries no such field, so a
/// host cannot supply one, which is the concrete reason this editor belongs to the plugin rather
/// than to the player (design system §7.1).
pub fn all_parameters(params: &MxmBucketDelayParams) -> Vec<Bound<'_>> {
    vec![
        Bound::new(
            "time",
            &params.time,
            "The delay time; longer times are darker and noisier, and turning it bends the pitch.",
        ),
        Bound::new("sync", &params.sync, super::binding::SYNC_DESCRIPTION),
        Bound::new(
            "change",
            &params.change,
            "What the echoes do when the tempo or note length changes.",
        ),
        Bound::new(
            "line",
            &params.line,
            "Which delay line: it sets how long, dark and gritty the echoes can be.",
        ),
        Bound::new(
            "filter",
            &params.filter,
            "How the echoes' tone follows the delay time.",
        ),
        Bound::new("reverse", &params.reverse, "Plays the echoes backwards."),
        Bound::new("tap1", &params.tap1, "The first echo's level.").labelled("1"),
        Bound::new("tap2", &params.tap2, "The second echo's level.").labelled("2"),
        Bound::new("tap3", &params.tap3, "The third echo's level.").labelled("3"),
        Bound::new("tap4", &params.tap4, "The fourth echo's level.").labelled("4"),
        Bound::new("tap5", &params.tap5, "The fifth echo's level.").labelled("5"),
        Bound::new(
            "tap6",
            &params.tap6,
            "The last echo's level; on its own, a plain single delay.",
        )
        .labelled("6"),
        Bound::new(
            "spread",
            &params.spread,
            "Spreads the earlier echoes out, or bunches them together.",
        ),
        Bound::new(
            "return",
            &params.return_mode,
            "What feeds back into the echoes.",
        ),
        Bound::new(
            "bias",
            &params.bias,
            "Centred, the echoes are cleanest; either way they get grittier.",
        )
        .bipolar(),
        Bound::new(
            "wobble",
            &params.wobble,
            "How much the delay time wobbles: chorus and vibrato.",
        ),
        Bound::new(
            "rate",
            &params.rate,
            "How fast it wobbles: tape flutter at one end, seasick at the other.",
        ),
        Bound::new(
            "ratesync",
            &params.rate_sync,
            super::binding::SYNC_DESCRIPTION,
        ),
        Bound::new(
            "feedback",
            &params.feedback,
            "How many times the echoes repeat; near the top they build up and sing on their own.",
        ),
        Bound::new(
            "mix",
            &params.mix,
            "The balance of the dry sound and the echoes; at zero the effect is off.",
        ),
        Bound::new("routing", &params.routing, "How the echoes sit in stereo."),
    ]
}

/// The delay in force, which is the knob while free and a subdivision when `Sync` follows the host.
///
/// One call into the parameters' own law rather than a second copy of it here.
fn effective_time(params: &MxmBucketDelayParams, tempo: Option<f64>) -> f32 {
    params.target_time(tempo)
}

/// What each option of a stepped control does, one sentence per cell in the parameter's own order
/// (design system §7.3; the owner, 2026-09-27: the cells of a row do not share one sentence).
fn details_of(id: &str) -> &'static [&'static str] {
    match id {
        "change" => &[
            "At a new tempo the echoes bend into their new time, as tape would.",
            "At a new tempo the echoes jump to their new time at once.",
        ],
        "line" => &[
            "The shortest line, up to about 50 ms: doubling and chorus-like echoes.",
            "Up to about 170 ms: slapback echoes.",
            "Up to about 200 ms, a little grittier.",
            "The longest, to about 400 ms: long echoes, with the most grit.",
        ],
        "filter" => &[
            "A fixed filter keeps short delays dark.",
            "The filter follows the delay time, so short delays stay bright.",
        ],
        "reverse" => &[
            "The echoes play as they came in.",
            "The echoes play backwards.",
        ],
        "return" => &[
            "Feeds the whole tap mix back: the repeats build into a wash.",
            "Feeds only the last tap back: the repeats stay clean under the taps.",
        ],
        "routing" => &[
            "One echo, the same on both sides.",
            "Two lines wobbling in opposite directions, for width.",
            "Echoes bounce between left and right.",
        ],
        _ => &[],
    }
}

fn binding_for<'a>(id: &str, params: &'a MxmBucketDelayParams) -> Bound<'a> {
    all_parameters(params)
        .into_iter()
        .find(|b| b.id == id)
        .expect("every drawn control is a bound parameter")
}

/// The cards' titles, in signal order.
pub const TITLES: [&str; 3] = ["Line", "Taps", "Character"];

/// What a leaf of this editor's cards draws. Hashed by what it names, which keeps its widget ids
/// stable when a card is re-paged.
#[derive(Clone, Debug, Hash)]
pub enum Leaf {
    Knob(&'static str, Size),
    /// A stepped parameter's segmented switch, beside a knob of this size or on its own line.
    Switch(&'static str, Option<Size>),
    /// A control's tempo sync, the quarter note beside it.
    Picture(&'static str),
    Constellation,
}

/// A segmented switch's cells: every option of the stepped parameter, as its own formatted text
/// (B4), so a host's list and the switch cannot disagree.
fn options(params: &MxmBucketDelayParams, id: &str) -> Vec<String> {
    let param = binding_for(id, params).param;
    let last = param
        .steps()
        .unwrap_or_else(|| unreachable!("`{id}` is not a switch"));
    (0..=last)
        .map(|option| param.format(option as f32 / last as f32))
        .collect()
}

/// A knob in a column `column` wide. A syncable control's column holds its free readings and its
/// divisions.
fn knob(params: &MxmBucketDelayParams, id: &'static str, size: Size, column: f32) -> Node<Leaf> {
    let bound = binding_for(id, params);
    let param = bound.param;
    let widest = match ladder_of(id) {
        Some(ladder) => super::binding::synced_widest(param, ladder.span),
        None => mxm_ui::control::widest_value(|n| param.format(n as f32)),
    };
    tree::leaf(
        Leaf::Knob(id, size),
        Kind::Knob {
            name: bound.painted().to_owned(),
            widest,
            size,
            column,
        },
    )
}

/// The ladder of the control `id`, if it has a tempo sync.
fn ladder_of(id: &str) -> Option<mxm_tempo::Ladder> {
    match id {
        "time" => Some(crate::params::TIME_SYNC),
        "rate" => Some(crate::params::RATE_SYNC),
        _ => None,
    }
}

/// **The division a synced knob reads**, or `None` for its free value: sync off, or no tempo
/// (`plans/plan-tempo-sync-controls.md`). Time's divisions are the fitted chip's, Rate's the knob's
/// own range.
fn shown_division(
    params: &MxmBucketDelayParams,
    id: &str,
    tempo: Option<f64>,
) -> Option<mxm_tempo::Division> {
    use nice_plug::prelude::Param as _;
    let (on, ladder, position, (lo, hi)) = match id {
        "time" => (
            params.sync.value(),
            crate::params::TIME_SYNC,
            params.time.unmodulated_normalized_value(),
            crate::params::time_bounds(params.line.value().line()),
        ),
        "rate" => (
            params.rate_sync.value(),
            crate::params::RATE_SYNC,
            params.rate.unmodulated_normalized_value(),
            (
                f64::from(params.rate.preview_plain(0.0)),
                f64::from(params.rate.preview_plain(1.0)),
            ),
        ),
        _ => return None,
    };
    if !on {
        return None;
    }
    ladder.shown(position, tempo, lo, hi)
}

/// A segmented switch, its painted label the parameter's name.
fn switch(params: &MxmBucketDelayParams, id: &'static str, beside: Option<Size>) -> Node<Leaf> {
    tree::leaf(
        Leaf::Switch(id, beside),
        Kind::Segmented {
            label: binding_for(id, params).param.name().to_owned(),
            options: options(params, id),
            beside,
        },
    )
}

/// Card `index`'s body, as a tree (plans/plan-layout-tree.md): described once, and that one
/// description is both measured — the card's floor and height — and drawn, leaf by leaf, through
/// the bindings ([`paint`]). Nothing in it follows the tempo: a synced knob's column already holds
/// every division it can read.
pub fn card(ui: &Ui, index: usize, params: &MxmBucketDelayParams) -> Node<Leaf> {
    let gap = ui.spacing().item_spacing.x;
    let wide = mxm_ui::control::knob_column(PRIMARY);
    match index {
        // Line: Time with its tempo sync beside it, then what a synced tail does at a change —
        // greyed while Time is free, where it does nothing — then the chip and the two switches
        // that share one cell (`plans/plan-tempo-sync-controls.md`).
        0 => tree::stack(vec![
            tree::row_gap(
                gap,
                vec![
                    knob(params, "time", PRIMARY, wide),
                    tree::switch_beside_knob(
                        PRIMARY,
                        tree::leaf(Leaf::Picture("sync"), Kind::SyncToggle),
                    ),
                ],
            ),
            if params.sync.value() {
                switch(params, "change", None)
            } else {
                tree::disabled(switch(params, "change", None))
            },
            switch(params, "line", None),
            // **Stacked, not side by side.** Two segmented switches on one row inside a card this
            // wide leaves the second one about seven points for its name, which egui then wraps
            // to one letter per line — a 7 x 98 point label, legible to nobody. The paint list is
            // where that showed up; it is invisible to a test that measures height. Stacked, the
            // two share one cell width so they read as a pair.
            tree::share(
                Share::Cells,
                tree::stack(vec![
                    switch(params, "filter", None),
                    switch(params, "reverse", None),
                ]),
            ),
        ]),
        // Taps: the constellation over the six faders, each in a column its circle and `SPACE_2`
        // wide, which is what keeps six on one line; then Spread with Return beside it.
        1 => tree::stack(vec![
            tree::leaf(
                Leaf::Constellation,
                Kind::Custom {
                    min_width: CONSTELLATION_MIN_WIDTH,
                    height: Height::Fixed(CONSTELLATION_HEIGHT),
                    fills: true,
                },
            ),
            mxm_ui::tree::knob_row(
                ui,
                ["tap1", "tap2", "tap3", "tap4", "tap5", "tap6"]
                    .into_iter()
                    .map(|id| (SECONDARY, knob(params, id, SECONDARY, 0.0)))
                    .collect(),
            ),
            tree::row_gap(
                gap,
                vec![
                    knob(
                        params,
                        "spread",
                        SECONDARY,
                        mxm_ui::control::knob_column(SECONDARY),
                    ),
                    tree::pad_all(0.0, SPACE_2, 0.0, switch(params, "return", Some(SECONDARY))),
                ],
            ),
        ]),
        // Character: the two a player performs with, the three that colour the line — Rate with its
        // tempo sync beside it — and the routing.
        _ => tree::stack(vec![
            mxm_ui::tree::knob_row(
                ui,
                vec![
                    (PRIMARY, knob(params, "feedback", PRIMARY, 0.0)),
                    (PRIMARY, knob(params, "mix", PRIMARY, 0.0)),
                ],
            ),
            tree::row_gap(
                gap,
                vec![
                    mxm_ui::tree::knob_row(
                        ui,
                        ["bias", "wobble", "rate"]
                            .into_iter()
                            .map(|id| (SECONDARY, knob(params, id, SECONDARY, 0.0)))
                            .collect(),
                    ),
                    tree::switch_beside_knob(
                        SECONDARY,
                        tree::leaf(Leaf::Picture("ratesync"), Kind::SyncToggle),
                    ),
                ],
            ),
            switch(params, "routing", None),
        ]),
    }
}

/// The authored cards, each floor computed from its tree in `ui`'s fonts. Also what the keyboard
/// cursor is given: `mxm_ui::navigation::paged` reads the plan's own order from the last frame's
/// report and falls back to these before one exists.
pub fn page_items(ui: &Ui, params: &MxmBucketDelayParams) -> Vec<mxm_ui::paging::Item<'static>> {
    use mxm_ui::paging::{Category, Item, Key};
    TITLES
        .iter()
        .enumerate()
        .map(|(index, title)| {
            let floor = tree::card_floor(ui, title, &card(ui, index, params));
            Item {
                key: Key(index as u64),
                // As wide as its controls and no wider (`plans/plan-editor-standard.md` A1).
                card: mxm_ui::flow::Card::new(title, floor).capped(floor),
                category: Category::Effects,
                kind: title,
            }
        })
        .collect()
}

/// Everything a leaf draws with, and the telemetry read once before the frame
/// (`plugins/AGENTS.md`: destructive telemetry is read once).
pub struct Live<'a, 'b> {
    pub params: &'a MxmBucketDelayParams,
    pub setter: &'a ParamSetter<'b>,
    pub text_entry: &'a mut HashMap<&'static str, Option<String>>,
    /// The constellation's brightness: the wet the audio thread actually added.
    pub ring: f32,
    /// The host tempo in force: a synced knob reads its division with one and its free value
    /// without, and the constellation draws the delay in force.
    pub tempo: Option<f64>,
}

/// Draws one leaf, in the `Ui` the tree bounded to `rect`, through the bindings — so the controls,
/// their gestures and their names are exactly what they were.
pub fn paint(ui: &mut Ui, tokens: &Tokens, leaf: &Leaf, rect: Rect, live: &mut Live<'_, '_>) {
    let params = live.params;
    match *leaf {
        // Synced, a knob reads the division in force; the host still reads its value.
        Leaf::Knob(id, size) => {
            let bound = binding_for(id, params);
            match shown_division(params, id, live.tempo) {
                Some(division) => bound.knob_with_reading(
                    ui,
                    tokens,
                    live.setter,
                    size,
                    rect.width(),
                    live.text_entry,
                    division.label(),
                ),
                None => bound.knob(ui, tokens, live.setter, size, rect.width(), live.text_entry),
            }
        }
        Leaf::Switch(id, beside) => {
            let bound = binding_for(id, params);
            let labels = options(params, id);
            let options: Vec<&str> = labels.iter().map(String::as_str).collect();
            segmented(
                ui,
                tokens,
                id,
                bound.param,
                &options,
                beside,
                details_of(id),
                live.setter,
            );
        }
        Leaf::Picture(id) => {
            super::binding::sync_picture(ui, tokens, id, binding_for(id, params).param, live.setter)
        }
        Leaf::Constellation => constellation(ui, tokens, params, live.ring, live.tempo),
    }
}

/// The whole view: three cards in wrapping rows, in signal order.
pub fn cards(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmBucketDelayParams,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    text_entry: &mut HashMap<&'static str, Option<String>>,
) -> f32 {
    // The tempo the audio thread last saw. `Time` picks a subdivision when `Sync` follows the host,
    // so the panel cannot say what the delay is without it.
    let tempo = telemetry.tempo();
    let items = page_items(ui, params);
    let text_editing = text_entry.values().any(Option::is_some);
    let mut live = Live {
        params,
        setter,
        text_entry,
        // Read once, before anything is drawn: the constellation's brightness is the wet the audio
        // thread actually added, and taking it while drawing would clear it for the next reader.
        ring: telemetry.take_ring(),
        tempo,
    };
    let report = mxm_ui::paging::editor::show(
        ui,
        tokens,
        &items,
        &[],
        text_editing,
        &mut |ui, index| card(ui, index, params),
        &mut |ui, _, leaf, rect| paint(ui, tokens, leaf, rect, &mut live),
    );

    // **Measured from the cards the flow actually drew**, not from `ui.min_rect()`: the flow lays
    // out through taffy, whose children do not necessarily extend the parent's rect, so the outer
    // measure under-reported by a card's worth of height and the fit test believed it.
    report
        .visible
        .iter()
        .map(|(_, rect)| rect.bottom())
        .fold(ui.min_rect().bottom(), f32::max)
}

/// **The constellation**: six marks on a line at their *actual* delay, against a scale that does
/// not rescale itself.
///
/// It is here because no number on a knob conveys the single fact that explains the effect — the
/// spacing is uneven, and deliberately so. Panasonic's own words for the six-tap part are that its
/// taps are *"not in multiple proportion with each other"*, and that is what makes the thing sound
/// like a space rather than an echo.
///
/// Three things move, and each teaches something:
///
/// - **Time stretches the whole set together**, because one clock drives everything;
/// - **Spread opens and closes it about the last mark**;
/// - **each mark's brightness is its fader**, so a preset's shape reads at a glance.
///
/// The geometry comes from the DSP's own tap table, so a display and a sound that disagreed would
/// be a compile error rather than a drawing mistake. The overall brightness is
/// `Telemetry::take_ring` — **the wet the audio thread actually added** — so a line that has
/// snapped to silence goes dark, and a `Line` switched mid-tail visibly dims through the change.
fn constellation(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmBucketDelayParams,
    ring: f32,
    tempo: Option<f64>,
) {
    let width = ui.available_width().max(CONSTELLATION_MIN_WIDTH);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, CONSTELLATION_HEIGHT),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, tokens.surface_2);

    let inset = SPACE_4;
    let left = rect.left() + inset;
    let right = rect.right() - inset;
    let span = (right - left).max(1.0);
    let baseline = rect.center().y + 6.0;

    // The scale does not rescale itself: it is a fixed window on time, so a longer delay pushes the
    // marks right rather than redrawing the same picture at a different label.
    let full_scale_s = crate::params::MAX_TIME_S;
    // **The delay in force, not the knob.** When `Sync` follows the host the knob is a subdivision
    // selector, so drawing the marks at `time.value()` would put the constellation somewhere the
    // sound is not.
    let time = effective_time(params, tempo);
    let spread = params.spread.value();

    painter.line_segment(
        [egui::pos2(left, baseline), egui::pos2(right, baseline)],
        egui::Stroke::new(1.0, tokens.border),
    );

    // How lit the whole picture is: what the audio thread actually produced, not what the controls
    // suggest it might.
    let lit = (ring * 6.0).clamp(0.12, 1.0);

    let taps = [
        params.tap1.value(),
        params.tap2.value(),
        params.tap3.value(),
        params.tap4.value(),
        params.tap5.value(),
        params.tap6.value(),
    ];

    let mut last_x = left;
    for i in 0..TAPS {
        let ratio = MN3011_TAP_STAGES[i] as f32 / MN3011_STAGES as f32;
        let stretched = (1.0 - spread * (1.0 - ratio)).clamp(0.0, 1.0);
        let at = time * stretched;
        let x = left + span * (at / full_scale_s).clamp(0.0, 1.0);
        let height = 10.0 + 26.0 * taps[i];
        let alpha = (0.18 + 0.82 * taps[i]) * lit;
        painter.line_segment(
            [egui::pos2(x, baseline), egui::pos2(x, baseline - height)],
            egui::Stroke::new(MARK_WIDTH, tokens.accent.gamma_multiply(alpha)),
        );
        if i == TAPS - 1 {
            last_x = x;
        }
    }

    // The loop, drawn as a return path from whichever taps `Return` selects, brightening with
    // Feedback — so the difference between Mix and Tail is a picture rather than a manual.
    let feedback = params.feedback.value();
    if feedback > 0.0 {
        let from = match params.return_mode.value() {
            crate::params::ReturnChoice::Tail => last_x,
            crate::params::ReturnChoice::Mix => {
                // The centre of what is actually open, which is what the loop is being fed.
                let total: f32 = taps.iter().sum();
                if total <= 0.0 {
                    last_x
                } else {
                    let mut weighted = 0.0;
                    for i in 0..TAPS {
                        let ratio = MN3011_TAP_STAGES[i] as f32 / MN3011_STAGES as f32;
                        let stretched = (1.0 - spread * (1.0 - ratio)).clamp(0.0, 1.0);
                        weighted += taps[i] * (time * stretched);
                    }
                    left + span * ((weighted / total) / full_scale_s).clamp(0.0, 1.0)
                }
            }
        };
        let arc = baseline + 18.0;
        let stroke = egui::Stroke::new(
            LOOP_THICKNESS,
            tokens.accent.gamma_multiply((0.2 + 0.8 * feedback) * lit),
        );
        painter.line_segment([egui::pos2(from, baseline), egui::pos2(from, arc)], stroke);
        painter.line_segment([egui::pos2(from, arc), egui::pos2(left, arc)], stroke);
        painter.line_segment([egui::pos2(left, arc), egui::pos2(left, baseline)], stroke);
    }

    let caption = format!("{:.0} ms", time * 1000.0);
    painter.text(
        egui::pos2(rect.left() + CAPTION_INSET, rect.bottom() - CAPTION_INSET),
        egui::Align2::LEFT_BOTTOM,
        caption,
        egui::FontId::proportional(CAPTION_FONT),
        tokens.text_secondary,
    );
    painter.text(
        egui::pos2(rect.right() - CAPTION_INSET, rect.bottom() - CAPTION_INSET),
        egui::Align2::RIGHT_BOTTOM,
        format!("{:.0} ms", full_scale_s * 1000.0),
        egui::FontId::proportional(CAPTION_FONT),
        tokens.text_secondary,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::prelude::Params;

    /// Every parameter is bound exactly once. One missing from here falls silently out of every
    /// preset, out of Init and out of the dirty baseline — none of which fails loudly.
    #[test]
    fn every_parameter_is_bound_exactly_once() {
        let params = MxmBucketDelayParams::default();
        let bound: Vec<&str> = all_parameters(&params).iter().map(|b| b.id).collect();
        for (id, _, _) in params.param_map() {
            assert!(
                bound.contains(&id.as_str()),
                "{id} is not bound to a control"
            );
        }
        assert_eq!(
            bound.len(),
            params.param_map().len(),
            "a control is bound that is not a parameter"
        );
        for (i, id) in bound.iter().enumerate() {
            assert!(!bound[i + 1..].contains(id), "{id} is bound twice");
        }
    }

    /// Every binding carries a sentence, because design system §7.1 requires one in every tooltip
    /// and CLAP has no field a host could supply it from.
    #[test]
    fn every_binding_carries_a_description() {
        let params = MxmBucketDelayParams::default();
        for bound in all_parameters(&params) {
            assert!(
                bound.description.len() > 20 && bound.description.ends_with('.'),
                "{} has no usable description",
                bound.id
            );
        }
    }

    /// The bias trimmer is the one bipolar control: its rest position is the centre, and a
    /// half-filled arc would read as "half on" where it means "trimmed".
    #[test]
    fn bias_is_the_only_bipolar_control() {
        let params = MxmBucketDelayParams::default();
        let bipolar: Vec<&str> = all_parameters(&params)
            .iter()
            .filter(|b| b.bipolar)
            .map(|b| b.id)
            .collect();
        assert_eq!(bipolar, vec!["bias"]);
    }
}
