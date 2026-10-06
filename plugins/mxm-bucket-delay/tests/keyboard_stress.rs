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
use nice_plug::prelude::*;

/// A host that applies what the editor asks for, as the real one does.
#[derive(Default)]
struct ApplyingHost;

impl nice_plug::context::gui::GuiContextInner for ApplyingHost {
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

    let press = |k: Key, m: Modifiers| vec![key(k, m, true, false), key(k, m, false, false)];
    let values = [
        Key::ArrowRight,
        Key::ArrowLeft,
        Key::ArrowUp,
        Key::ArrowDown,
        Key::Home,
        Key::End,
    ];
    for _card in 0..12 {
        for _parameter in 0..16 {
            for value in values {
                frame(press(value, Modifiers::NONE));
                frame(Vec::new());
            }
            // A held key: a press, two repeats, a release.
            frame(vec![key(Key::ArrowUp, Modifiers::NONE, true, false)]);
            frame(vec![key(Key::ArrowUp, Modifiers::NONE, true, true)]);
            frame(vec![key(Key::ArrowUp, Modifiers::NONE, true, true)]);
            frame(vec![key(Key::ArrowUp, Modifiers::NONE, false, false)]);
            frame(press(Key::ArrowRight, Modifiers::COMMAND));
            frame(Vec::new());
            frame(press(Key::ArrowDown, Modifiers::COMMAND));
            frame(Vec::new());
        }
        frame(press(Key::ArrowRight, Modifiers::SHIFT));
        frame(Vec::new());
        frame(press(Key::ArrowDown, Modifiers::SHIFT));
        frame(Vec::new());
    }
}
