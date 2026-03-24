use std::{collections::BTreeSet, sync::Mutex, thread};

use rdev::{listen, Button, EventType};
use serde::Serialize;
use tauri::{menu::MenuItem, AppHandle, Emitter, Manager, Wry};

use crate::app::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum InputEvent {
    KeyEvent { pressed: bool, name: String },
    MouseButtonEvent { pressed: bool, button: MouseButton },
    MouseMoveEvent { x: f64, y: f64 },
    MouseWheelEvent { delta_x: i64, delta_y: i64 },
}

#[derive(Debug, Clone, Serialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other,
}

pub fn map_mouse_button(button: Button) -> MouseButton {
    match button {
        Button::Left => MouseButton::Left,
        Button::Right => MouseButton::Right,
        Button::Middle => MouseButton::Middle,
        _ => MouseButton::Other,
    }
}

fn shortcut_key_name(key_name: &str) -> &str {
    match key_name {
        "Shift" | "ShiftLeft" | "ShiftRight" => "Shift",
        "Control" | "ControlLeft" | "ControlRight" => "Control",
        "Alt" | "AltGr" => "Alt",
        "Meta" | "MetaLeft" | "MetaRight" => "Meta",
        _ => key_name,
    }
}

fn shortcut_signature(keys: &[String]) -> BTreeSet<String> {
    keys.iter()
        .map(|key_name| shortcut_key_name(key_name).to_string())
        .collect()
}

fn matches_toggle_shortcut(toggle_shortcut: &[String], pressed_keys: &[String]) -> bool {
    shortcut_signature(toggle_shortcut) == shortcut_signature(pressed_keys)
}

pub fn start_listener(app_handle: AppHandle, toggle_menu_item: MenuItem<Wry>) {
    thread::spawn(move || {
        println!("Starting global input listener...");

        if let Err(err) = listen(move |event| {
            // get app state
            let state = app_handle.state::<Mutex<AppState>>();
            let mut app_state = state.lock().unwrap();

            // track pressed keys
            if let EventType::KeyPress(key) = event.event_type {
                let key_name = format!("{:?}", key);
                // If the name contains parenthesis (like "RawKey(123)", "Unknown()"), ignore it.
                if key_name.contains('(') {
                    return;
                }
                // if key is already marked as pressed, ignore repeat
                if app_state.pressed_keys.contains(&key_name) {
                    return;
                }
                // record key as pressed
                app_state.pressed_keys.push(key_name);
                // check if toggle shortcut is pressed
                if matches_toggle_shortcut(&app_state.toggle_shortcut, &app_state.pressed_keys) {
                    app_state.toggle_listener(&app_handle, &toggle_menu_item);

                    if !app_state.listening {
                        // emit key releases for all pressed keys
                        for key_name in &app_state.pressed_keys {
                            app_handle
                                .emit_to(
                                    "main",
                                    "input-event",
                                    InputEvent::KeyEvent {
                                        pressed: false,
                                        name: key_name.clone(),
                                    },
                                )
                                .unwrap()
                        }
                    }
                }
            } else if let EventType::KeyRelease(key) = event.event_type {
                let key_name = format!("{:?}", key);
                if key_name.contains('(') {
                    return;
                }
                // remove key from pressed keys
                app_state.pressed_keys.retain(|k| k != &key_name);
            }

            // emit event if listening
            if !app_state.listening {
                return;
            }
            let input_event = match event.event_type {
                EventType::KeyPress(key) => Some(InputEvent::KeyEvent {
                    pressed: true,
                    name: format!("{:?}", key),
                }),
                EventType::KeyRelease(key) => Some(InputEvent::KeyEvent {
                    pressed: false,
                    name: format!("{:?}", key),
                }),
                EventType::ButtonPress(button) => Some(InputEvent::MouseButtonEvent {
                    pressed: true,
                    button: map_mouse_button(button),
                }),
                EventType::ButtonRelease(button) => Some(InputEvent::MouseButtonEvent {
                    button: map_mouse_button(button),
                    pressed: false,
                }),
                EventType::MouseMove { x, y } => {
                    // Convert Physical -> Logical
                    #[cfg(target_os = "macos")]
                    let (logical_x, logical_y) = (
                        x - app_state.monitor_position.0 as f64,
                        y - app_state.monitor_position.1 as f64,
                    );

                    #[cfg(not(target_os = "macos"))]
                    let (logical_x, logical_y) = {
                        let (offset_x, offset_y) = app_state.monitor_position;
                        (x - offset_x as f64, y - offset_y as f64)
                    };

                    Some(InputEvent::MouseMoveEvent {
                        x: logical_x,
                        y: logical_y,
                    })
                }
                EventType::Wheel { delta_x, delta_y } => {
                    Some(InputEvent::MouseWheelEvent { delta_x, delta_y })
                }
            };

            app_handle.emit("input-event", input_event).unwrap();
        }) {
            eprintln!("rdev listen failed: {:?}", err);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::matches_toggle_shortcut;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn matches_left_or_right_alt_for_logical_alt_shortcuts() {
        assert!(matches_toggle_shortcut(
            &strings(&["Alt", "F10"]),
            &strings(&["AltGr", "F10"])
        ));
        assert!(matches_toggle_shortcut(
            &strings(&["Alt", "F10"]),
            &strings(&["Alt", "F10"])
        ));
    }

    #[test]
    fn matches_shortcuts_regardless_of_modifier_side_or_order() {
        assert!(matches_toggle_shortcut(
            &strings(&["Shift", "F10"]),
            &strings(&["F10", "ShiftRight"])
        ));
        assert!(matches_toggle_shortcut(
            &strings(&["ControlLeft", "KeyK"]),
            &strings(&["KeyK", "ControlRight"])
        ));
    }

    #[test]
    fn does_not_match_when_extra_non_alias_keys_are_pressed() {
        assert!(!matches_toggle_shortcut(
            &strings(&["Shift", "F10"]),
            &strings(&["ShiftLeft", "F10", "KeyA"])
        ));
    }
}
