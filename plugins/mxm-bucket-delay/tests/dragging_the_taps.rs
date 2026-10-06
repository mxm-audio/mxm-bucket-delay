//! A hand on the six tap faders, through the shipped panel.
//!
//! Reported 2026-09-06: the host disappeared while the six tap faders were being played with. No
//! Rust panic reached its log and Windows recorded no fault — which is what a panic *inside a
//! plugin's editor* looks like from outside, because it unwinds across the plugin boundary and
//! takes the process with it.
//!
//! `editor.rs`'s own tests paint the panel at every setting of every control and find nothing. What
//! they cannot do is **interact**: a drag runs the gesture path — `begin_set_parameter`, a run of
//! `set_parameter_normalized`, `end_set_parameter` — plus the drag tracking, the text-entry state
//! and the host's answers coming back. This file puts a pointer on each of the six knobs and moves
//! it, which is the thing that was actually being done.

use std::collections::HashMap;
use std::sync::Mutex;

use egui::{ThemePreference, vec2};
use mxm_bucket_delay::editor::{self, PresetUi};
use mxm_bucket_delay::params::MxmBucketDelayParams;
use mxm_bucket_delay::telemetry::Telemetry;
use nice_plug::prelude::*;

/// A host that applies what the editor asks for, as the real one does — so a drag actually moves
/// the parameter and the next frame draws the moved value.
#[derive(Default)]
struct ApplyingHost(Mutex<Vec<&'static str>>);

impl nice_plug::context::gui::GuiContextInner for ApplyingHost {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _param: nice_plug::params::internals::ParamPtr) {
        self.0.lock().unwrap().push("begin");
    }
    unsafe fn raw_set_parameter_normalized(
        &self,
        param: nice_plug::params::internals::ParamPtr,
        value: f32,
    ) {
        self.0.lock().unwrap().push("set");
        unsafe {
            param._internal_set_normalized_value(value);
        }
    }
    unsafe fn raw_end_set_parameter(&self, _param: nice_plug::params::internals::ParamPtr) {
        self.0.lock().unwrap().push("end");
    }
    fn get_state(&self) -> PluginState {
        PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        }
    }
    fn set_state(&self, _: PluginState) {}
}

/// Drags every control the panel draws, one at a time, in both themes.
///
/// **Every control, not only the six.** The report named the tap faders, but a defect that only
/// appears under a gesture is not likely to care which knob the gesture is on, and the six are the
/// cheapest thing to be wrong about — six of the twenty draw from one shared code path.
#[test]
fn dragging_every_control_through_the_shipped_panel_is_survivable() {
    for theme in [ThemePreference::Light, ThemePreference::Dark] {
        let params = MxmBucketDelayParams::default();
        let telemetry = Telemetry::default();
        let host = ApplyingHost::default();
        let mut text_entry = HashMap::new();
        let mut presets = PresetUi::at(mxm_bucket_delay::preset::Library::at(None), &params);
        let mut nav = mxm_ui::navigation::State::default();

        let mut harness = egui_kittest::Harness::builder()
            .with_size(vec2(1348.0, 460.0))
            .build_ui(|ui| {
                mxm_ui::theme::apply(ui.ctx());
                mxm_ui::typography::apply(ui.ctx());
                ui.ctx().set_theme(theme);
                editor::panel(
                    ui,
                    &params,
                    &telemetry,
                    &ParamSetter::new(&host),
                    &mut text_entry,
                    &mut presets,
                    &mut nav,
                );
            });

        // **Fixed steps, never `run`.** The panel requests a repaint every frame — the
        // constellation dims with a decaying tail, which is not something egui can see coming — so
        // "settled" never arrives and `run` gives up at its step cap. `apps/mxm-player`'s own
        // harness records the same rule for the same reason.
        harness.run_steps(3);

        // The panel is drawn; now sweep a pointer across the whole of it, pressing and dragging,
        // which lands on every control in turn without needing to know where any of them is.
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1348.0, 460.0));
        let mut y = 90.0;
        while y < rect.bottom() {
            let mut x = 20.0;
            while x < rect.right() {
                let at = egui::pos2(x, y);
                harness
                    .input_mut()
                    .events
                    .push(egui::Event::PointerMoved(at));
                harness.input_mut().events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                });
                harness.step();
                // A drag: up, down, and well past the control's own travel in both directions.
                for dy in [-40.0f32, 80.0, -200.0, 400.0] {
                    let to = egui::pos2(x, y + dy);
                    harness
                        .input_mut()
                        .events
                        .push(egui::Event::PointerMoved(to));
                    harness.step();
                }
                harness.input_mut().events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                });
                harness.step();
                x += 44.0;
            }
            y += 40.0;
        }

        // The host saw gestures, and every one of them is closed: an unclosed gesture leaves a
        // DAW's automation lane latched, which is silent until somebody records over it.
        let seen = host.0.lock().unwrap().clone();
        let begins = seen.iter().filter(|e| **e == "begin").count();
        let ends = seen.iter().filter(|e| **e == "end").count();
        assert!(begins > 0, "{theme:?}: no control was actually dragged");
        assert_eq!(
            begins, ends,
            "{theme:?}: {begins} gestures began, {ends} ended"
        );
    }
}
