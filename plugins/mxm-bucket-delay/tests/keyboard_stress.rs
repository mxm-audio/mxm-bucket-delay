//! Every keyboard gesture on every parameter, through the shipped panel.
//!
//! Reported 2026-09-23: MXM Player disappeared while the keyboard cursor was being used in this
//! editor and mxm-mono-08's, with no panic text and no Windows fault — which is what a panic
//! inside a plugin's editor looks like from outside. The coverage check proves the cursor reaches
//! every parameter; this presses every key the cursor answers to on every one of them.

use std::collections::HashMap;

use egui::{Event, Key, Modifiers};
use mxm_bucket_delay::editor::{self, PresetUi};
use mxm_bucket_delay::params::MxmBucketDelayParams;
use mxm_bucket_delay::telemetry::Telemetry;
use mxm_plugin_test::keyboard_checks::{COARSE, MICRO, OUT, VALUE, VIEW, key_of};
use nice_plug::prelude::*;

/// A host that applies what the editor asks for, as the real one does.
#[derive(Default)]
struct ApplyingHost;

impl nice_plug::context::gui::GuiContextInner for ApplyingHost {
    // A test double has no host to ask for a restart (nice-plug 0.4).
    fn request_restart(&self) {}
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _param: nice_plug::params::internals::ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(
        &self,
        param: nice_plug::params::internals::ParamPtr,
        value: f32,
    ) {
        assert!(value.is_finite(), "the editor sent a non-finite value");
        unsafe {
            param._internal_set_normalized_value(value);
        }
    }
    unsafe fn raw_end_set_parameter(&self, _param: nice_plug::params::internals::ParamPtr) {}
    fn get_state(&self) -> PluginState {
        PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        }
    }
    fn set_state(&self, _: PluginState) {}
}

fn key(key: Key, modifiers: Modifiers, pressed: bool, repeat: bool) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat,
        modifiers,
    }
}

#[test]
fn every_keyboard_gesture_on_every_parameter_leaves_the_editor_standing() {
    let params = MxmBucketDelayParams::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost;
    let setter = ParamSetter::new(&host);
    let ctx = egui::Context::default();
    mxm_ui::theme::apply(&ctx);
    mxm_ui::typography::apply(&ctx);
    let mut text_entry = HashMap::new();
    let mut presets = PresetUi::at(mxm_bucket_delay::preset::Library::at(None), &params);
    let mut nav = mxm_ui::navigation::State::default();

    let mut frame = |events: Vec<Event>| {
        let modifiers = events
            .iter()
            .rev()
            .find_map(|event| match event {
                Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut events = events;
        events.insert(0, Event::ModifiersChanged(modifiers));
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            editor::panel(
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
    };
    for _ in 0..4 {
        frame(Vec::new());
    }

    // The keyboard language (every editor's since 2026-10-08), pressed as jobs on the default
    // keymap's keys: VALUE, COARSE, MICRO (← → snap to the next line, 2026-10-09), OUT, Escape
    // BACK, and VIEW + an arrow to the next card.
    let none = Modifiers::NONE;
    let tap = |k: Key| vec![key(k, none, true, false), key(k, none, false, false)];
    let taps = |keys: &[Key]| keys.iter().flat_map(|&k| tap(k)).collect::<Vec<_>>();
    let arrows = [
        Key::ArrowRight,
        Key::ArrowLeft,
        Key::ArrowUp,
        Key::ArrowDown,
    ];
    for _card in 0..12 {
        for _parameter in 0..16 {
            for arrow in arrows {
                frame(taps(&[key_of(VALUE), arrow, key_of(OUT)]));
                frame(Vec::new());
            }
            for arrow in [Key::ArrowUp, Key::ArrowDown] {
                frame(taps(&[key_of(VALUE), key_of(COARSE), arrow, key_of(OUT)]));
                frame(Vec::new());
            }
            for arrow in [Key::ArrowRight, Key::ArrowLeft] {
                frame(taps(&[key_of(VALUE), key_of(MICRO), arrow, key_of(OUT)]));
                frame(Vec::new());
            }
            for end in [Key::Home, Key::End] {
                frame(tap(end));
                frame(Vec::new());
            }
            // A held VALUE + ↑: a press, two repeats, a release, then BACK cancels the gesture.
            frame(vec![
                key(key_of(VALUE), none, true, false),
                key(Key::ArrowUp, none, true, false),
            ]);
            frame(vec![key(Key::ArrowUp, none, true, true)]);
            frame(vec![key(Key::ArrowUp, none, true, true)]);
            frame(vec![key(Key::ArrowUp, none, false, false)]);
            frame(tap(Key::Escape));
            frame(vec![key(key_of(VALUE), none, false, false)]);
            frame(tap(Key::Delete));
            frame(Vec::new());
            frame(tap(Key::ArrowRight));
            frame(Vec::new());
            frame(tap(Key::ArrowDown));
            frame(Vec::new());
        }
        frame(taps(&[key_of(VIEW), Key::ArrowRight]));
        frame(Vec::new());
        frame(taps(&[key_of(VIEW), Key::ArrowDown]));
        frame(Vec::new());
    }
}
