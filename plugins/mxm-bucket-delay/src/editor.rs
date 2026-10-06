//! mxm-bucket-delay's editor.
//!
//! Built to `docs/briefs/mxm-bucket-delay.md`, which is the gating document — this module
//! implements it and does not re-decide it. It follows the shape `mxm-chorus-06` set for an
//! effect's editor: **one view, no view bar**, the collection accent rather than an identity hue,
//! the window's width set by the app bar, and the display beside the controls rather than under
//! them. Where this one differs is the display, which is the tank itself.
//!
//! # It is a panel, not a window
//!
//! [`panel`] takes a `Ui` and draws into it. It does not create a window, run an event loop, or own
//! a swapchain.
//!
//! # Gestures
//!
//! Every edit is bracketed: `begin_set_parameter`, `set_parameter_normalized`, `end_set_parameter`,
//! in exactly one place — [`binding::Bound::apply`]. An unclosed gesture leaves a host's automation
//! lane latched, and it breaks the player's step editing outright.
//!
//! # No developer channel
//!
//! It arrives as MIDI CC and an effect has no note port. See `telemetry.rs`.

pub mod binding;
pub mod sections;

use std::collections::HashMap;
use std::sync::Arc;

use egui::Ui;
use mxm_ui::space::SPACE_5;
use mxm_ui::theme::Tokens;
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::*;
use nice_plug_egui::{EguiEditorState, NiceEguiApp, create_egui_editor};

use crate::params::MxmBucketDelayParams;
use crate::telemetry::Telemetry;

/// The opening size: the quarter-4K budget hugged to the three cards on one row
/// (`plans/plan-editor-standard.md` F1), which `tests::the_opening_size_is_the_budget_hugged`
/// holds; `tests::the_panel_fits_the_editor` pins the height. The app bar compacts to that width
/// (design system §3.1).
const REFERENCE: (u32, u32) = (880, 472);

/// The narrowest the window may be. Below this the card's controls stop being usable, which no
/// arrangement fixes — there is only one card here, so there is nothing to rearrange (§4.3).
const MINIMUM: (u32, u32) = (562, 400);

/// Builds the editor. Called from `Plugin::editor`.
pub fn create(
    params: Arc<MxmBucketDelayParams>,
    telemetry: Arc<Telemetry>,
) -> Option<MxmBucketDelayEditor> {
    let state = EguiEditorState::from_size(
        nice_plug::editor::dpi::LogicalSize::new(REFERENCE.0, REFERENCE.1),
        1.0,
    );

    create_egui_editor(
        state,
        nice_plug_egui::RepaintNotifier::new(),
        nice_plug_egui::EguiNiceSettings {
            title: "mxm-bucket-delay".to_owned(),
            // **Resizable**, as every editor in the collection is (`plugins/AGENTS.md`).
            //
            // There is no flow here and there does not need to be: this effect is **one card**, and
            // a single card has no row to wrap into. What resizing buys it is a window a tiling
            // manager can size, with the card taking whatever width it is given; the floor is the
            // width below which the card's own controls stop being usable (§4.3).
            resize_hint: ResizeHint {
                size_constraints: nice_plug::editor::SizeConstraints::min_logical_size(
                    nice_plug::editor::dpi::LogicalSize::new(MINIMUM.0 as f32, MINIMUM.1 as f32),
                ),
                ..ResizeHint::RESIZABLE
            },
            ..Default::default()
        },
        MxmBucketDelayApp::new(params, telemetry),
    )
}

/// The editor type the plugin exposes.
pub type MxmBucketDelayEditor = nice_plug_egui::EguiEditor<MxmBucketDelayApp>;

/// Where the panel records the bottom of its content, for the fit test to read.
///
/// **Not `globally_used_rect`**: the central panel fills the window whatever is in it, so that
/// measure only exceeds the window once something is already cut off.
pub(crate) fn content_bottom_id() -> egui::Id {
    egui::Id::new("mxm-bucket-delay-content-bottom")
}

/// The editor's own state: what the plugin does not own and the host does not need.
pub struct MxmBucketDelayApp {
    params: Arc<MxmBucketDelayParams>,
    telemetry: Arc<Telemetry>,
    /// Set in `build`, because that is where nice-plug hands it over.
    gui_context: Option<GuiContext>,
    /// Open text-entry buffers, keyed by parameter id.
    text_entry: HashMap<&'static str, Option<String>>,
    /// The preset library and everything the browser needs across frames.
    presets: PresetUi,
    /// Where the keyboard is: a card, and a parameter inside it. Transient, like the text
    /// buffers — it is not a parameter and nothing durable reads it.
    nav: mxm_ui::navigation::State,
}

pub use mxm_preset::PresetUi;

impl MxmBucketDelayApp {
    pub fn new(params: Arc<MxmBucketDelayParams>, telemetry: Arc<Telemetry>) -> Self {
        let params_for_presets = Arc::clone(&params);
        Self {
            params,
            telemetry,
            gui_context: None,
            text_entry: HashMap::new(),
            presets: PresetUi::new(params_for_presets.as_ref()),
            nav: mxm_ui::navigation::State::default(),
        }
    }
}

impl NiceEguiApp for MxmBucketDelayApp {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        mxm_ui::theme::apply(&egui_ctx);
        mxm_ui::typography::apply(&egui_ctx);
        // Light by default, overridable with `MXM_EDITOR_THEME`. The reasoning, and why the
        // default is not `System`, lives on `mxm_ui::theme::preference`.
        egui_ctx.set_theme(mxm_ui::theme::preference());
        self.gui_context = Some(nice_gui_ctx);
        Ok(())
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut nice_plug_egui::Frame) {
        let Some(gui_context) = self.gui_context.clone() else {
            return;
        };
        panel(
            ui,
            &self.params,
            &self.telemetry,
            &gui_context.param_setter(),
            &mut self.text_entry,
            &mut self.presets,
            &mut self.nav,
        );
    }

    fn editor_closed(&mut self) {
        self.gui_context = None;
    }
}

/// The whole editor, as a panel.
pub fn panel(
    ui: &mut Ui,
    params: &MxmBucketDelayParams,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    text_entry: &mut HashMap<&'static str, Option<String>>,
    presets: &mut PresetUi,
    nav: &mut mxm_ui::navigation::State,
) {
    let tokens = &tokens_for(ui);

    // **The springs brighten and dim without input, so the frames have to come without input too.**
    // egui repaints when something happens; a tail decaying is not something egui can see.
    ui.ctx().request_repaint();

    let peak = telemetry.take_peak();
    let clipped = telemetry.clipped();
    // One question, and both layers suspend on it: the paging renderer's `hold` and the cursor's
    // `inert` both ask whether another surface owns this frame's keyboard.
    let busy = presets.holds_the_keyboard() || text_entry.values().any(Option::is_some);
    mxm_ui::paging::editor::hold(ui.ctx(), busy);
    // **The cursor moves before anything is drawn**, so a navigation arrow is consumed here rather
    // than also walking egui's own focus ring. It reads the registry and the exact card rectangles
    // the previous frame built, and navigates the paging plan's own order.
    mxm_ui::navigation::paged(ui.ctx(), nav, busy);

    mxm_ui::AppBar::new("mxm-bucket-delay").show_with(
        ui,
        tokens,
        |ui| mxm_preset::ui::preset_row(ui, tokens, params, setter, presets),
        |ui| {
            if mxm_ui::shell::level_meter(ui, tokens, peak, clipped) {
                telemetry.clear_clip();
            }
            mxm_ui::shell::zoom_control(ui);

            // §3.1 slot 5, and the same place the player keeps it: at the left end of the bar's
            // right-hand group. What the person picks is remembered for every MXM editor, so the
            // next one to open agrees with this one.
            mxm_ui::shell::editor_theme_control(ui);
        },
    );

    mxm_preset::ui::overlays(ui, tokens, params, setter, presets);

    // The shared renderer derives any required navigation below the app bar.
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(tokens.canvas)
                .inner_margin(egui::Margin::same(SPACE_5 as i8)),
        )
        .show(ui, |ui| {
            let bottom = sections::cards(ui, tokens, params, telemetry, setter, text_entry);
            ui.data_mut(|d| d.insert_temp(content_bottom_id(), bottom + SPACE_5));
        });
}

/// The collection's tokens, unchanged. **No identity accent**: §5.3 gives each *instrument* a hue
/// so a rack of them is tellable apart; an effect is told apart by its name in a chain.
fn tokens_for(ui: &Ui) -> Tokens {
    if ui.visuals().dark_mode {
        mxm_ui::DARK
    } else {
        mxm_ui::LIGHT
    }
}

/// The paging items as the editor computes them, from a context set up as an editor's is — three
/// passes in, so the weighted font cuts are bound — for tests, which have no editor `Ui` to hand.
#[cfg(test)]
pub(crate) fn test_items(params: &MxmBucketDelayParams) -> Vec<mxm_ui::paging::Item<'static>> {
    let ctx = egui::Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    let mut items = Vec::new();
    for _ in 0..3 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            items = sections::page_items(ui, params);
        });
        output.textures_delta.clear();
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::params::internals::ParamPtr;
    use nice_plug::prelude::{PluginApi, PluginState};

    /// The editor reports edits through a `ParamSetter`; laying it out makes none.
    struct NoHost;

    impl nice_plug::context::gui::GuiContextInner for NoHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
        unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {}
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    /// Lays the panel out headlessly and returns the context and the bottom of its content.
    fn lay_out(width: f32, height: f32, mix: f32) -> (egui::Context, f32) {
        let ctx = egui::Context::default();
        mxm_ui::theme::apply(&ctx);
        mxm_ui::typography::apply(&ctx);
        ctx.set_theme(egui::ThemePreference::Light);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);

        let params = MxmBucketDelayParams::default();
        {
            use nice_plug::params::{InternalParamMut, Param};
            let normalised = params.mix.preview_normalized(mix);
            unsafe {
                let _ = params.mix._internal_set_normalized_value(normalised);
                params.mix._internal_update_smoother(48_000.0, true);
            }
        }
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        // A library rooted nowhere: a layout test must never touch the real config directory.
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        };

        for _ in 0..3 {
            let mut output = ctx.run_ui(input.clone(), |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            });
            output.textures_delta.clear();
        }
        let bottom = ctx
            .memory(|m| m.data.get_temp::<f32>(content_bottom_id()))
            .expect("the panel records where its content ends");
        (ctx, bottom)
    }

    use mxm_plugin_test::keyboard_checks;
    use mxm_plugin_test::opening_size;

    /// **The editor opens at the quarter-4K budget, hugged** (`plans/plan-editor-standard.md` F1):
    /// `REFERENCE` is derived, not typed, and this holds it.
    #[test]
    fn the_opening_size_is_the_budget_hugged() {
        let params = MxmBucketDelayParams::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        opening_size::is_the_budget_hugged(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &REVEAL,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    /// **The app bar holds in the narrowest window**: its `…` menu whole and nothing drawn over
    /// anything else, from `MINIMUM` up (`opening_size::bar_holds_from_the_minimum`).
    #[test]
    fn the_app_bar_holds_in_the_minimum_window() {
        let params = MxmBucketDelayParams::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        opening_size::bar_holds_from_the_minimum(
            egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    /// What this editor keeps behind a disclosure, opened so the reachability check sees it.
    /// Nothing here: every control is on a card.
    const REVEAL: fn(&egui::Context) = |_| {};

    /// The rollout's own failure mode: a control whose `navigation::at` scope was forgotten paints
    /// exactly as before and is simply unreachable from the keyboard. Nothing else would say so.
    #[test]
    fn the_keyboard_cursor_reaches_and_operates_every_parameter() {
        let params = MxmBucketDelayParams::default();
        let telemetry = Telemetry::default();
        let host = keyboard_checks::Recorder::default();
        let setter = ParamSetter::new(&host);
        let ids: Vec<&str> = sections::all_parameters(&params)
            .iter()
            .map(|bound| bound.id)
            .collect();
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        keyboard_checks::the_cursor_reaches_and_operates(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &test_items(&params),
            keyboard_checks::Coverage::Exactly(&ids),
            &REVEAL,
            &host,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }

    /// **The scar both mono editors carry, turned into a number.** A height guessed before the
    /// panel existed, and controls cut off the bottom of a window that cannot be resized.
    #[test]
    fn the_panel_fits_the_editor() {
        let (_, used) = lay_out(
            REFERENCE.0 as f32,
            REFERENCE.1 as f32,
            crate::params::DEFAULT_MIX,
        );
        eprintln!("the panel needs {used} points of height");
        assert!(
            used <= REFERENCE.1 as f32,
            "the panel needs {used} points of height in a {} point window; it will be clipped",
            REFERENCE.1
        );
        assert!(
            used > REFERENCE.1 as f32 - 120.0,
            "the panel needs only {used} points in a {} point window; the window is taller than it has to be",
            REFERENCE.1
        );
    }

    /// The Off state is a *layout* no-op: the display changes what it paints, never how much room
    /// it takes, or the window would resize itself when the level reached zero.
    #[test]
    fn switching_off_does_not_move_anything() {
        let (_, engaged) = lay_out(
            REFERENCE.0 as f32,
            REFERENCE.1 as f32,
            crate::params::DEFAULT_MIX,
        );
        let (_, off) = lay_out(REFERENCE.0 as f32, REFERENCE.1 as f32, 0.0);
        assert!(
            (engaged - off).abs() < 0.5,
            "the panel is {engaged} points engaged and {off} off; the Off state must not reflow"
        );
    }

    /// Every word the panel paints, in draw order, with `MXM_DUMP=1` printing where each one landed.
    ///
    /// ```text
    /// MXM_DUMP=1 cargo test -p mxm-bucket-delay the_panel_paints -- --nocapture
    /// ```
    ///
    /// That listing is what found the empty editor: every control was being painted at x = 1112 to
    /// 3284 in a 1020-point window.
    ///
    /// Read off egui's own shape list rather than off the layout's bookkeeping, because those are
    /// two different claims: a panel can allocate the right amount of height and paint nothing into
    /// it, which is what an empty editor *is*.
    fn painted_text(width: f32, height: f32) -> Vec<String> {
        let ctx = egui::Context::default();
        mxm_ui::theme::apply(&ctx);
        mxm_ui::typography::apply(&ctx);
        ctx.set_theme(egui::ThemePreference::Light);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);

        let params = MxmBucketDelayParams::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        };

        let mut words = Vec::new();
        for pass in 0..3 {
            let mut output = ctx.run_ui(input.clone(), |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            });
            if pass == 2 {
                for clipped in &output.shapes {
                    collect_text(&clipped.shape, &mut words);
                }
            }
            output.textures_delta.clear();
        }
        words
    }

    fn collect_text(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                if std::env::var("MXM_DUMP").is_ok() {
                    let r = text.galley.rect.translate(text.pos.to_vec2());
                    eprintln!(
                        "{:>8.1},{:>7.1} {:>6.1}x{:<6.1} {:?}",
                        r.left(),
                        r.top(),
                        r.width(),
                        r.height(),
                        text.galley.text()
                    );
                }
                out.push(text.galley.text().to_owned())
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text(shape, out);
                }
            }
            _ => {}
        }
    }

    /// Every painted label with the box it was laid out in.
    fn painted_boxes(width: f32, height: f32) -> Vec<(String, egui::Rect)> {
        let ctx = egui::Context::default();
        mxm_ui::theme::apply(&ctx);
        mxm_ui::typography::apply(&ctx);
        ctx.set_theme(egui::ThemePreference::Light);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);

        let params = MxmBucketDelayParams::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        };

        let mut found = Vec::new();
        for pass in 0..3 {
            let mut output = ctx.run_ui(input.clone(), |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            });
            if pass == 2 {
                for clipped in &output.shapes {
                    collect_boxes(&clipped.shape, &mut found);
                }
            }
            output.textures_delta.clear();
        }
        found
    }

    fn collect_boxes(shape: &egui::epaint::Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push((
                text.galley.text().to_owned(),
                text.galley.rect.translate(text.pos.to_vec2()),
            )),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_boxes(shape, out);
                }
            }
            _ => {}
        }
    }

    /// **No label is squeezed into a column of letters.** A control given too little width does not
    /// disappear — egui wraps its name one character per line, and the row still reports a sensible
    /// height, so nothing downstream notices. Two segmented switches sharing a row inside one card
    /// left the second about seven points for `Reverse`, which came out 7 x 98.
    #[test]
    fn no_painted_label_is_wrapped_into_a_column_of_letters() {
        for (text, rect) in painted_boxes(REFERENCE.0 as f32, REFERENCE.1 as f32) {
            if text.chars().count() < 2 {
                continue;
            }
            assert!(
                rect.height() <= 3.0 * rect.width().max(1.0),
                "{text:?} was painted {:.1} wide and {:.1} tall: it has been squeezed into a                  column of letters",
                rect.width(),
                rect.height()
            );
        }
    }

    /// Nothing is painted outside the window it opens at. The fit test measures height; this is the
    /// other axis, and the one the empty editor was hiding in.
    #[test]
    fn nothing_is_painted_outside_the_reference_window() {
        let (w, h) = (REFERENCE.0 as f32, REFERENCE.1 as f32);
        for (text, rect) in painted_boxes(w, h) {
            assert!(
                rect.right() <= w && rect.bottom() <= h && rect.left() >= 0.0,
                "{text:?} was painted at {rect:?}, outside the {w} x {h} window"
            );
        }
    }

    /// **The panel drawn across the whole parameter space.**
    ///
    /// Reported 2026-09-06: the host disappeared while the six tap faders were being played with,
    /// with no panic in its log and no Windows fault record. That is what a panic *inside a plugin
    /// editor* looks like from outside — it unwinds into the host across the plugin boundary and
    /// takes the process with it, silently. So every control is swept over its whole travel and the
    /// panel drawn at each setting: an index, a division or an `expect` that only bites at one
    /// value has to bite here instead.
    #[test]
    fn the_panel_draws_at_every_setting_of_every_control() {
        use nice_plug::params::{InternalParamMut, Param};

        let params = MxmBucketDelayParams::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();

        let ctx = egui::Context::default();
        mxm_ui::theme::apply(&ctx);
        mxm_ui::typography::apply(&ctx);
        ctx.set_theme(egui::ThemePreference::Light);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);

        let floats: [&FloatParam; 12] = [
            &params.time,
            &params.feedback,
            &params.mix,
            &params.tap1,
            &params.tap2,
            &params.tap3,
            &params.tap4,
            &params.tap5,
            &params.tap6,
            &params.spread,
            &params.bias,
            &params.wobble,
        ];

        // Every setting each control reaches, and the extremes of all of them together — which is
        // where a six-fader sweep ends up, and where a sum or a ratio is most likely to be zero.
        for step in 0..=20 {
            let v = step as f32 / 20.0;
            for param in floats {
                unsafe {
                    let _ = param._internal_set_normalized_value(v);
                    param._internal_update_smoother(48_000.0, true);
                }
            }
            // The ring is what the constellation is lit by, and it comes off the audio thread: an
            // extreme there has to be drawable too.
            telemetry.publish_ring(match step % 4 {
                0 => 0.0,
                1 => 1e-9,
                2 => 1.0,
                _ => 40.0,
            });

            for ret in [
                crate::params::ReturnChoice::Mix,
                crate::params::ReturnChoice::Tail,
            ] {
                unsafe {
                    let n = params.return_mode.preview_normalized(ret);
                    let _ = params.return_mode._internal_set_normalized_value(n);
                }
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
                    )),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    panel(
                        ui,
                        &params,
                        &telemetry,
                        &setter,
                        &mut text_entry,
                        &mut presets,
                        &mut nav,
                    );
                });
                output.textures_delta.clear();
            }
        }
    }

    /// The window sizes a host may actually give the editor, including ones far narrower and far
    /// wider than it opens at. A layout that only survives its own reference size is a layout that
    /// will meet a host that disagrees.
    #[test]
    fn the_panel_draws_at_every_window_size_a_host_may_give_it() {
        for (w, h) in [
            (MINIMUM.0 as f32, MINIMUM.1 as f32),
            (200.0, 150.0),
            (REFERENCE.0 as f32, REFERENCE.1 as f32),
            (3000.0, 2000.0),
            (640.0, 1400.0),
            (1900.0, 300.0),
        ] {
            let words = painted_text(w, h);
            assert!(!words.is_empty(), "nothing was painted at {w} x {h}");
        }
    }

    /// **The editor draws its controls.** The fit test above measures how much room the panel
    /// takes; this one measures what it puts in it, which is the difference between a laid-out
    /// panel and a visible one.
    #[test]
    fn the_panel_paints_every_card_and_every_control() {
        let words = painted_text(REFERENCE.0 as f32, REFERENCE.1 as f32);
        assert!(
            !words.is_empty(),
            "the panel painted no text at all: the editor is empty"
        );
        for expected in [
            // The three cards.
            "Line",
            "Taps",
            "Character",
            // One control from each, by its own label: a tap paints its number, the card says the
            // rest (B1).
            "Time",
            "1",
            "6",
            "Spread",
            "Return",
            "Bias",
            "Wobble",
            "Rate",
            "Feedback",
            "Mix",
            "Routing",
            // What a synced tail does at a change. Time's and Rate's syncs are pictures, named
            // only to a screen reader.
            "Change",
            "Filter",
            "Reverse",
        ] {
            assert!(
                words.iter().any(|w| w == expected),
                "the editor never painted {expected:?}; it painted {words:?}"
            );
        }
    }

    /// The window's floor is **one card**, not three, because the cards wrap. A window minimum
    /// exists to stop a card being drawn narrower than its own controls, and that is one card —
    /// `apps/mxm-layout-lab`'s finding (in the private archive), and the rule `mxm_ui::flow` is
    /// built on.
    #[test]
    fn the_minimum_window_holds_the_widest_card() {
        let cards: Vec<_> = test_items(&MxmBucketDelayParams::default())
            .iter()
            .map(|item| item.card)
            .collect();
        let floor = mxm_ui::flow::minimum_width(&cards) + 2.0 * SPACE_5;
        assert!(
            MINIMUM.0 as f32 >= floor,
            "the window may shrink to {} points and the widest card needs {floor}",
            MINIMUM.0
        );
    }

    /// The reference window puts all three cards on one row, which is the shape the brief asks for:
    /// Line, Taps, Character, left to right, following the signal.
    #[test]
    fn the_reference_window_puts_the_three_cards_on_one_row() {
        let floors: f32 = test_items(&MxmBucketDelayParams::default())
            .iter()
            .map(|item| item.card.floor)
            .sum();
        let needed = floors + 2.0 * mxm_ui::flow::GAP + 2.0 * SPACE_5;
        assert!(
            REFERENCE.0 as f32 >= needed,
            "one row needs {needed} points and the window opens at {}",
            REFERENCE.0
        );
        // And not much more than that: a window wider than its content is empty space.
        assert!(
            REFERENCE.0 as f32 <= needed + 40.0,
            "the window opens at {} where one row needs {needed}",
            REFERENCE.0
        );
    }

    /// Straight into a parameter: a check has no host.
    fn set<P: nice_plug::params::InternalParamMut>(param: &P, plain: P::Plain) {
        unsafe {
            let _ = param._internal_set_plain_value(plain);
            param._internal_update_smoother(48_000.0, true);
        }
    }

    /// Every continuous parameter, for the states that take them all to one end.
    fn floats(params: &MxmBucketDelayParams) -> [&FloatParam; 13] {
        [
            &params.time,
            &params.feedback,
            &params.mix,
            &params.tap1,
            &params.tap2,
            &params.tap3,
            &params.tap4,
            &params.tap5,
            &params.tap6,
            &params.spread,
            &params.bias,
            &params.wobble,
            &params.rate,
        ]
    }

    /// Takes `params` and a tempo to one state of the structural-state matrix.
    fn enter(params: &MxmBucketDelayParams, state: &str) -> (Option<f64>, f32) {
        use crate::params::{ChangeChoice, ReturnChoice};
        use nice_plug::params::InternalParamMut;
        let normalised = |param: &FloatParam, value: f32| unsafe {
            let _ = param._internal_set_normalized_value(value);
            param._internal_update_smoother(48_000.0, true);
        };
        match state {
            "every control at its top, ringing" => {
                floats(params).into_iter().for_each(|p| normalised(p, 1.0));
                set(&params.return_mode, ReturnChoice::Tail);
                (None, 40.0)
            }
            "every control at its bottom" => {
                floats(params).into_iter().for_each(|p| normalised(p, 0.0));
                (None, 0.0)
            }
            "synced with no host tempo" => {
                set(&params.sync, true);
                set(&params.rate_sync, true);
                (None, 0.5)
            }
            "synced to a host tempo" => {
                set(&params.sync, true);
                set(&params.change, ChangeChoice::Snap);
                set(&params.rate_sync, true);
                (Some(120.0), 0.5)
            }
            _ => (None, 0.0),
        }
    }

    /// Every card, in every state that changes what it holds or paints, passes the layout tree's
    /// checks (plans/plan-layout-tree.md §4.3, `tree_checks::card`): its computed floor holds its
    /// content with nothing painted outside the card, the content floor is exact, the height its
    /// tree states is the height it draws, and every leaf stays in the room it was given.
    ///
    /// The states are this editor's structural-state matrix. The cards have no route, disclosure
    /// or reserved alternative; what changes is what a synced knob reads — its free value, or a
    /// division of the host's tempo — whether Change is greyed, and what the constellation paints:
    /// every control at its top with the ring overdriven and the loop drawn from the last tap, and
    /// every control at its bottom.
    #[test]
    fn every_card_passes_the_tree_checks_in_every_state() {
        for state in [
            "init",
            "every control at its top, ringing",
            "every control at its bottom",
            "synced with no host tempo",
            "synced to a host tempo",
        ] {
            let params = MxmBucketDelayParams::default();
            let (tempo, ring) = enter(&params, state);
            let floors: Vec<f32> = test_items(&params)
                .iter()
                .map(|item| item.card.floor)
                .collect();
            let host = NoHost;
            let setter = ParamSetter::new(&host);
            for (index, floor) in floors.into_iter().enumerate() {
                let mut text_entry = HashMap::new();
                let mut live = sections::Live {
                    params: &params,
                    setter: &setter,
                    text_entry: &mut text_entry,
                    ring,
                    tempo,
                };
                tree_checks::card(
                    &|_| {},
                    state,
                    sections::TITLES[index],
                    floor,
                    &|ui| sections::card(ui, index, &params),
                    &mut |ui, leaf, rect| {
                        sections::paint(ui, &mxm_ui::LIGHT, leaf, rect, &mut live);
                    },
                );
            }
        }
    }

    use mxm_plugin_test::tree_checks;

    /// Every page at the opening size, light and dark, for the owner's review of the layout-tree
    /// conversion (plans/plan-layout-tree.md §4.3): `target/layout-tree/mxm-bucket-delay/<tag>/`,
    /// where `MXM_PICTURES` names the tag — `before` on the unconverted editor, `after` on the tree.
    ///
    /// `MXM_PICTURES=after cargo test -p mxm-bucket-delay --lib tree_pictures -- --ignored`
    #[test]
    #[ignore = "renders through wgpu; run by hand"]
    fn tree_pictures() {
        let tag = std::env::var("MXM_PICTURES").unwrap_or_else(|_| "after".to_owned());
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/layout-tree/mxm-bucket-delay")
            .join(tag);
        let params = MxmBucketDelayParams::default();
        let telemetry = Telemetry::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(crate::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();
        tree_checks::pictures(
            &|_| {},
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &dir,
            &mut |ui| {
                panel(
                    ui,
                    &params,
                    &telemetry,
                    &setter,
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            },
        );
    }
}
