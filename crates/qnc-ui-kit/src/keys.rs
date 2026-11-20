//! The window's key presses as catalog chords. The catalog names keys as on a keyboard; this is
//! the one table from the drawing library's keys to those names. A key the table does not
//! know gives no chord. Presses are read, never consumed: the panel in focus decides what a
//! chord means, and a text field that has the keyboard keeps the keys for typing.

use egui::{Context, Event, Key};
use qnc_actions::Chord;

fn name(key: Key) -> Option<&'static str> {
    use Key::*;
    Some(match key {
        Enter => "Enter",
        Escape => "Escape",
        Space => "Space",
        Tab => "Tab",
        Backspace => "Backspace",
        Delete => "Delete",
        Home => "Home",
        End => "End",
        PageUp => "PageUp",
        PageDown => "PageDown",
        ArrowLeft => "Left",
        ArrowRight => "Right",
        ArrowUp => "Up",
        ArrowDown => "Down",
        Slash => "/",
        Backslash => "\\",
        OpenBracket => "[",
        CloseBracket => "]",
        Comma => ",",
        Period => ".",
        Minus => "-",
        Plus => "+",
        Equals => "=",
        Semicolon => ";",
        Quote => "'",
        Backtick => "`",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        F11 => "F11",
        F12 => "F12",
        A => "A",
        B => "B",
        C => "C",
        D => "D",
        E => "E",
        F => "F",
        G => "G",
        H => "H",
        I => "I",
        J => "J",
        K => "K",
        L => "L",
        M => "M",
        N => "N",
        O => "O",
        P => "P",
        Q => "Q",
        R => "R",
        S => "S",
        T => "T",
        U => "U",
        V => "V",
        W => "W",
        X => "X",
        Y => "Y",
        Z => "Z",
        Num0 => "0",
        Num1 => "1",
        Num2 => "2",
        Num3 => "3",
        Num4 => "4",
        Num5 => "5",
        Num6 => "6",
        Num7 => "7",
        Num8 => "8",
        Num9 => "9",
        _ => return None,
    })
}

/// The chords pressed in this frame, in order. Repeats of a held key count as presses.
pub fn chords(ctx: &Context) -> Vec<Chord> {
    ctx.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Key { key, pressed: true, modifiers, .. } => name(*key).map(|key| Chord {
                    key: key.to_string(),
                    ctrl: modifiers.ctrl || modifiers.command,
                    shift: modifiers.shift,
                    alt: modifiers.alt,
                }),
                _ => None,
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chord_of_the_built_in_catalog_names_a_key_this_kit_can_give() {
        let catalog = qnc_actions::catalog();
        let keys: Vec<&'static str> = [
            Key::Enter, Key::Escape, Key::Space, Key::Tab, Key::Backspace, Key::Delete, Key::Home, Key::End,
            Key::PageUp, Key::PageDown, Key::ArrowLeft, Key::ArrowRight, Key::ArrowUp, Key::ArrowDown, Key::Slash,
            Key::Backslash, Key::OpenBracket, Key::CloseBracket, Key::Comma, Key::Period, Key::Minus, Key::Plus,
            Key::Equals, Key::Semicolon, Key::Quote, Key::Backtick, Key::F1, Key::F2, Key::F3, Key::F4, Key::F5,
            Key::F6, Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12, Key::A, Key::B, Key::C, Key::D, Key::E,
            Key::F, Key::G, Key::H, Key::I, Key::J, Key::K, Key::L, Key::M, Key::N, Key::O, Key::P, Key::Q, Key::R,
            Key::S, Key::T, Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z, Key::Num0, Key::Num1, Key::Num2,
            Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9,
        ]
        .into_iter()
        .filter_map(name)
        .collect();
        for preset in catalog.presets() {
            for scope in catalog.scopes(preset) {
                for (chord, action) in catalog.bindings(preset, scope) {
                    assert!(keys.contains(&chord.key.as_str()), "{preset}/{scope}: {chord} ({action}) names a key the kit cannot give");
                }
            }
        }
    }
}
