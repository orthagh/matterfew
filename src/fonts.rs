//! The interface fonts, embedded so that every desktop shows the same text:
//! Nunito Sans for everything that is read, Bricolage Grotesque for titles
//! (files and licences in `assets/fonts`).
//!
//! Static weights only: GPUI draws a variable font at its regular weight,
//! which leaves no bold.

use std::borrow::Cow;
use std::io::Read as _;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::{App, SharedString, Styled, px, relative};

const TEXT_FAMILY: &str = "Nunito Sans";
const TITLE_FAMILY: &str = "Bricolage Grotesque";

const FILES: [&[u8]; 7] = [
    include_bytes!("../assets/fonts/NunitoSans-Regular.ttf.gz"),
    include_bytes!("../assets/fonts/NunitoSans-Italic.ttf.gz"),
    include_bytes!("../assets/fonts/NunitoSans-SemiBold.ttf.gz"),
    include_bytes!("../assets/fonts/NunitoSans-Bold.ttf.gz"),
    include_bytes!("../assets/fonts/NunitoSans-BoldItalic.ttf.gz"),
    include_bytes!("../assets/fonts/BricolageGrotesque-SemiBold.ttf.gz"),
    include_bytes!("../assets/fonts/BricolageGrotesque-Bold.ttf.gz"),
];

static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Hands the embedded fonts to the text system. If that fails, the system
/// font stays in place.
pub fn install(cx: &mut App) {
    let fonts: Option<Vec<Cow<'static, [u8]>>> = FILES
        .iter()
        .map(|compressed| {
            let mut bytes = Vec::new();
            flate2::read::GzDecoder::new(*compressed).read_to_end(&mut bytes).ok()?;
            Some(Cow::Owned(bytes))
        })
        .collect();
    let installed = fonts.is_some_and(|fonts| cx.text_system().add_fonts(fonts).is_ok());
    INSTALLED.store(installed, Ordering::Relaxed);
}

/// The font of everything that is read, once installed.
pub fn text() -> Option<SharedString> {
    INSTALLED.load(Ordering::Relaxed).then(|| TEXT_FAMILY.into())
}

/// Titles: the name of a channel or of a window, section headings. Author
/// names are not titles: they come back with every message, in the text font.
pub trait TitleFont: Styled + Sized {
    fn title_font(self) -> Self {
        if INSTALLED.load(Ordering::Relaxed) { self.font_family(TITLE_FAMILY) } else { self }
    }
}

impl<T: Styled> TitleFont for T {}

/// The type scale: four sizes, one role each.
///
/// Timestamps, counters, metadata.
pub const CAPTION: f32 = 12.;
/// Panels, dialogs, section headings: what frames the reading.
pub const SECONDARY: f32 = 13.;
/// What is read: messages, the names of their authors, and the lists of
/// channels and of people.
pub const BODY: f32 = 15.;
/// The line of a message.
pub const BODY_LINE: f32 = 22.;
/// Titles.
pub const TITLE: f32 = 18.;

pub trait TypeScale: Styled + Sized {
    fn caption_size(self) -> Self {
        self.text_size(px(CAPTION))
    }

    fn secondary_size(self) -> Self {
        self.text_size(px(SECONDARY))
    }

    /// The line height follows the size, so that a heading or a code block
    /// inside a message keeps its proportions.
    fn body_size(self) -> Self {
        self.text_size(px(BODY)).line_height(relative(BODY_LINE / BODY))
    }

    fn title_size(self) -> Self {
        self.text_size(px(TITLE))
    }
}

impl<T: Styled> TypeScale for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_fonts_decompress_to_truetype() {
        for compressed in FILES {
            let mut bytes = Vec::new();
            flate2::read::GzDecoder::new(compressed).read_to_end(&mut bytes).unwrap();
            // A TrueType file starts with its version, 1.0.
            assert_eq!(bytes[..4], [0, 1, 0, 0]);
        }
    }
}
