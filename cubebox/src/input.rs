//! Four-button input with debounce and long-press detection.
//!
//! Buttons are wired to GND with the internal pull-up enabled, so a pressed
//! button reads low. Polling at 20 ms is plenty for a timer UI.

use esp_idf_svc::hal::gpio::{Input, PinDriver};

use crate::state::{AppState, Page, RoomAction};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// Start / stop; long press = brand new session.
    Start,
    /// Next attempt.
    Next,
    /// Cycle +2 / DNF on the last result.
    Penalty,
    /// Cycle pages.
    Menu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEvent {
    Press(Button),
    LongPress(Button),
}

const DEBOUNCE_MS: u64 = 25;
const LONG_PRESS_MS: u64 = 700;

struct Debounced {
    /// Last raw (undebounced) level; true = pressed.
    raw: bool,
    raw_since: u64,
    /// Debounced level.
    stable: bool,
    pressed_at: u64,
    long_fired: bool,
}

impl Debounced {
    fn new() -> Self {
        Self {
            raw: false,
            raw_since: 0,
            stable: false,
            pressed_at: 0,
            long_fired: false,
        }
    }
}

pub struct Buttons {
    pins: [PinDriver<'static, Input>; 4],
    state: [Debounced; 4],
}

impl Buttons {
    pub fn new(pins: [PinDriver<'static, Input>; 4]) -> Self {
        Self {
            pins,
            state: [
                Debounced::new(),
                Debounced::new(),
                Debounced::new(),
                Debounced::new(),
            ],
        }
    }

    /// Sample the pins and return any debounced events.
    pub fn poll(&mut self, now_ms: u64) -> Vec<InputEvent> {
        let mut out = Vec::new();
        for i in 0..4 {
            let pressed = self.pins[i].is_low();
            let dev = &mut self.state[i];
            if pressed != dev.raw {
                dev.raw = pressed;
                dev.raw_since = now_ms;
            }
            if dev.raw != dev.stable && now_ms.saturating_sub(dev.raw_since) >= DEBOUNCE_MS {
                dev.stable = dev.raw;
                if dev.stable {
                    dev.pressed_at = now_ms;
                    dev.long_fired = false;
                    out.push(InputEvent::Press(Self::button(i)));
                }
            }
            if dev.stable
                && !dev.long_fired
                && now_ms.saturating_sub(dev.pressed_at) >= LONG_PRESS_MS
            {
                dev.long_fired = true;
                out.push(InputEvent::LongPress(Self::button(i)));
            }
        }
        out
    }

    fn button(i: usize) -> Button {
        match i {
            0 => Button::Start,
            1 => Button::Next,
            2 => Button::Penalty,
            _ => Button::Menu,
        }
    }
}

/// Apply an input event to the shared state.
pub fn apply(st: &mut AppState, ev: InputEvent, seed: u64) {
    // The room page has its own button mapping.
    if st.page == Page::Room {
        match ev {
            InputEvent::Press(Button::Start) => {
                let act = match st.room.as_ref() {
                    None => RoomAction::Create,
                    Some(r) if r.hosting && r.phase == "LOBBY" => RoomAction::Begin,
                    Some(_) => RoomAction::Create,
                };
                st.room_action = Some(act);
                st.bump();
            }
            InputEvent::Press(Button::Next) => {
                st.room_action = Some(RoomAction::Join);
                st.bump();
            }
            InputEvent::Press(Button::Penalty) => {
                st.room_action = Some(RoomAction::Leave);
                st.bump();
            }
            InputEvent::Press(Button::Menu) => {
                st.page = st.page.next();
                st.bump();
            }
            InputEvent::LongPress(Button::Menu) => {
                st.page = Page::Timer;
                st.bump();
            }
            _ => {}
        }
        return;
    }

    match ev {
        InputEvent::Press(Button::Start) => {
            // A short press starts a new attempt whenever none is armed.
            if st.session.is_none()
                || st.phase() == shared::timer::Phase::Done
                || st.phase() == shared::timer::Phase::Scrambling
            {
                st.new_session(seed);
            }
        }
        InputEvent::Press(Button::Next) => st.new_session(seed),
        InputEvent::Press(Button::Penalty) => st.cycle_penalty(),
        InputEvent::Press(Button::Menu) => {
            st.page = st.page.next();
            st.bump();
        }
        InputEvent::LongPress(Button::Start) => {
            // Long press: hard reset of the session.
            st.session = None;
            st.bump();
        }
        InputEvent::LongPress(Button::Next) => {
            // Long press: cycle the training group (CFOP / EG / 澶嶅師缁冧範).
            st.group_type = match st.group_type {
                shared::timer::GroupType::Basic => shared::timer::GroupType::Special,
                shared::timer::GroupType::Special => shared::timer::GroupType::Free,
                shared::timer::GroupType::Free => shared::timer::GroupType::Basic,
            };
            st.group_id = match (st.group_type, st.cube_type) {
                (shared::timer::GroupType::Basic, shared::timer::CubeType::Three) => {
                    shared::timer::GROUP_CFOP
                }
                (shared::timer::GroupType::Basic, shared::timer::CubeType::Two) => {
                    shared::timer::GROUP_EG
                }
                (shared::timer::GroupType::Special, _) => shared::timer::GROUP_F2L_STD,
                (shared::timer::GroupType::Free, shared::timer::CubeType::Three) => {
                    shared::timer::GROUP_SOLVE_3
                }
                (shared::timer::GroupType::Free, shared::timer::CubeType::Two) => {
                    shared::timer::GROUP_SOLVE_2
                }
            };
            st.bump();
        }
        InputEvent::LongPress(Button::Penalty) | InputEvent::LongPress(Button::Menu) => {
            // Reserved.
        }
    }
}

