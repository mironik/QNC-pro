//! The theme is data: colours and measures read from `contracts/ui/theme.json` (the graphite
//! palette and chrome measures of the old QNC shell contract, mirrored literally), built into the
//! program. No block below a panel knows a colour type of a drawing library; the drawing kit
//! turns `Rgb` into its own colour. There is one theme per process.

use std::sync::OnceLock;

use serde::Deserialize;

pub const CONTRACT: &str = include_str!("../../../contracts/ui/theme.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub [u8; 3]);

impl Rgb {
    /// `#rrggbb`
    pub fn parse(text: &str) -> Result<Rgb, String> {
        let hex = text.strip_prefix('#').ok_or_else(|| format!("colour {text}: no #"))?;
        if hex.len() != 6 {
            return Err(format!("colour {text}: not #rrggbb"));
        }
        let part = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).map_err(|e| format!("colour {text}: {e}"));
        Ok(Rgb([part(0)?, part(2)?, part(4)?]))
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Rgb, D::Error> {
        let text = String::deserialize(deserializer)?;
        Rgb::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Colours {
    pub background: Rgb,
    pub panel: Rgb,
    /// A raised control on a panel (a button, a picker head).
    pub raised: Rgb,
    pub border: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    /// The fill of the one strong button and the pressed state.
    pub accent: Rgb,
    pub focus: Rgb,
    pub danger: Rgb,
    pub done: Rgb,
    pub marked: Rgb,
    pub playhead: Rgb,
    pub button: Rgb,
    pub button_on: Rgb,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Metrics {
    pub font_size: f32,
    pub font_timecode: f32,
    /// A chrome row (title, tabs, dock header).
    pub row_height: f32,
    /// A control in a chrome row.
    pub control_height: f32,
    pub gap: f32,
    pub pad_y: f32,
    pub margin: f32,
    pub focus_width: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    pub colours: Colours,
    pub metrics: Metrics,
}

impl Theme {
    pub fn parse(json: &str) -> Result<Theme, String> {
        serde_json::from_str(json).map_err(|e| qnc_text::fill("theme.unreadable", &[("error", &e.to_string())]))
    }
}

static THEME: OnceLock<Theme> = OnceLock::new();

/// The theme of this process.
pub fn theme() -> &'static Theme {
    THEME.get_or_init(|| Theme::parse(CONTRACT).expect("the built-in theme contract is valid; its test says so"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_theme_parses_and_a_colour_is_rrggbb() {
        let theme = Theme::parse(CONTRACT).unwrap();
        assert_eq!(theme.colours.playhead, Rgb([0xed, 0xed, 0xed]));
        assert!(theme.metrics.row_height > theme.metrics.font_size);
        assert!(Rgb::parse("e6c84a").is_err());
        assert!(Rgb::parse("#e6c8").is_err());
        assert_eq!(Rgb::parse("#e6c84a").unwrap(), Rgb([0xe6, 0xc8, 0x4a]));
    }
}
