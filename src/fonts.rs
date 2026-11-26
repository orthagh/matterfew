//! The interface fonts, embedded so that every desktop shows the same text:
//! Nunito Sans for everything that is read, Bricolage Grotesque for titles
//! (files and licences in `assets/fonts`).
//!
//! Static weights only: GPUI draws a variable font at its regular weight,
//! which leaves no bold.

use std::borrow::Cow;
use std::io::Read as _;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::{App, SharedString, Styled};

const TEXT: &str = "Nunito Sans";
const TITLE: &str = "Bricolage Grotesque";

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
    INSTALLED.load(Ordering::Relaxed).then(|| TEXT.into())
}

/// Titles: the name of a channel or of a window, section headings. Author
/// names are not titles: they come back with every message, in the text font.
pub trait TitleFont: Styled + Sized {
    fn title_font(self) -> Self {
        if INSTALLED.load(Ordering::Relaxed) { self.font_family(TITLE) } else { self }
    }
}

impl<T: Styled> TitleFont for T {}

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
