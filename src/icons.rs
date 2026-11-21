//! Icônes dessinées par le code (pas de fichiers d'images à livrer) :
//! icône de l'application, icône de la zone de notification avec son état,
//! pastille de la barre des tâches avec le nombre de mentions.
//! Toutes renvoient des pixels RGBA, ligne par ligne.

// Hors Windows, une partie de ce module ne sert pas (correcteur de Windows,
// icônes de la barre des tâches).
#![cfg_attr(not(windows), allow(dead_code))]

/// Bleu de l'icône (bouton Mattermost « Denim »).
const BLUE: [u8; 3] = [0x1c, 0x58, 0xd9];
/// Rouge des pastilles de mentions.
const RED: [u8; 3] = [0xd2, 0x4b, 0x4e];

/// Police 3 × 5 pour les pastilles : chiffres, « + » et « ! ».
fn glyph(c: char) -> Option<[u8; 5]> {
    // Chaque ligne : 3 bits, le bit de poids fort à gauche.
    Some(match c {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        '!' => [0b010, 0b010, 0b010, 0b000, 0b010],
        _ => return None,
    })
}

struct Canvas {
    size: u32,
    px: Vec<u8>,
}

impl Canvas {
    fn new(size: u32) -> Self {
        Self { size, px: vec![0; (size * size * 4) as usize] }
    }

    /// Mélange une couleur avec une couverture `alpha` (0..1), pour des bords lissés.
    fn blend(&mut self, x: u32, y: u32, rgb: [u8; 3], alpha: f32) {
        if x >= self.size || y >= self.size || alpha <= 0.0 {
            return;
        }
        let i = ((y * self.size + x) * 4) as usize;
        let a = alpha.min(1.0);
        let dst_a = self.px[i + 3] as f32 / 255.0;
        let out_a = a + dst_a * (1.0 - a);
        for c in 0..3 {
            let src = rgb[c] as f32;
            let dst = self.px[i + c] as f32;
            self.px[i + c] = ((src * a + dst * dst_a * (1.0 - a)) / out_a.max(1e-6)) as u8;
        }
        self.px[i + 3] = (out_a * 255.0) as u8;
    }

    /// Disque lissé de centre (cx, cy) et de rayon r.
    fn disc(&mut self, cx: f32, cy: f32, r: f32, rgb: [u8; 3]) {
        for y in 0..self.size {
            for x in 0..self.size {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                self.blend(x, y, rgb, r + 0.5 - d);
            }
        }
    }

    /// Rectangle arrondi lissé.
    fn rounded_rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, radius: f32, rgb: [u8; 3]) {
        for y in 0..self.size {
            for x in 0..self.size {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let qx = (x0 + radius - px).max(px - (x1 - radius)).max(0.0);
                let qy = (y0 + radius - py).max(py - (y1 - radius)).max(0.0);
                let outside = (qx * qx + qy * qy).sqrt() - radius;
                let inside_box = px >= x0 - 1.0 && px <= x1 + 1.0 && py >= y0 - 1.0 && py <= y1 + 1.0;
                if inside_box {
                    self.blend(x, y, rgb, 0.5 - outside);
                }
            }
        }
    }

    /// Triangle (pointe de la bulle), sans lissage.
    fn triangle(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32), rgb: [u8; 3]) {
        let edge = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| (r.0 - p.0) * (q.1 - p.1) - (r.1 - p.1) * (q.0 - p.0);
        for y in 0..self.size {
            for x in 0..self.size {
                let p = (x as f32 + 0.5, y as f32 + 0.5);
                let (w0, w1, w2) = (edge(b, c, p), edge(c, a, p), edge(a, b, p));
                if (w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0) || (w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0) {
                    self.blend(x, y, rgb, 1.0);
                }
            }
        }
    }

    /// Texte centré avec la police 3 × 5, agrandie de `scale`.
    fn text(&mut self, text: &str, cx: f32, cy: f32, scale: u32, rgb: [u8; 3]) {
        let glyphs: Vec<[u8; 5]> = text.chars().filter_map(glyph).collect();
        if glyphs.is_empty() {
            return;
        }
        let width = (glyphs.len() as u32 * 4 - 1) * scale;
        let height = 5 * scale;
        let x0 = (cx - width as f32 / 2.0).round() as i64;
        let y0 = (cy - height as f32 / 2.0).round() as i64;
        for (gi, g) in glyphs.iter().enumerate() {
            for (row, bits) in g.iter().enumerate() {
                for col in 0..3 {
                    if bits & (0b100 >> col) == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let x = x0 + ((gi as u32 * 4 + col) * scale + dx) as i64;
                            let y = y0 + (row as u32 * scale + dy) as i64;
                            if x >= 0 && y >= 0 {
                                self.blend(x as u32, y as u32, rgb, 1.0);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Icône de l'application : carré arrondi bleu et bulle de discussion blanche.
pub fn app_icon(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut c = Canvas::new(size);
    c.rounded_rect(0.0, 0.0, s, s, s * 0.22, BLUE);
    c.rounded_rect(s * 0.2, s * 0.24, s * 0.8, s * 0.66, s * 0.12, [255, 255, 255]);
    c.triangle((s * 0.3, s * 0.6), (s * 0.48, s * 0.64), (s * 0.27, s * 0.8), [255, 255, 255]);
    for i in 0..3 {
        c.disc(s * (0.36 + 0.14 * i as f32), s * 0.45, s * 0.045, BLUE);
    }
    c.px
}

/// Icône de la zone de notification : l'icône de l'application, avec une
/// pastille rouge s'il y a des mentions, ou blanche s'il y a des non-lus.
pub fn tray_icon(size: u32, mentions: i64, unread: bool) -> Vec<u8> {
    let mut c = Canvas { size, px: app_icon(size) };
    let s = size as f32;
    if mentions > 0 || unread {
        let (cx, cy, r) = (s * 0.78, s * 0.22, s * 0.22);
        c.disc(cx, cy, r + s * 0.05, [0x14, 0x21, 0x3e]);
        c.disc(cx, cy, r, if mentions > 0 { RED } else { [255, 255, 255] });
    }
    c.px
}

/// Texte de la pastille : « 1 » à « 99 », puis « 99+ » ; « ! » si la session
/// a expiré.
pub fn badge_text(mentions: i64, expired: bool) -> String {
    if expired {
        "!".into()
    } else if mentions > 99 {
        "99+".into()
    } else {
        mentions.to_string()
    }
}

/// Pastille de la barre des tâches (16 px pour Windows) : disque rouge
/// avec le nombre de mentions, ou petit point pour de simples non-lus.
pub fn overlay_badge(size: u32, mentions: i64, unread: bool, expired: bool) -> Option<Vec<u8>> {
    let s = size as f32;
    let mut c = Canvas::new(size);
    if mentions > 0 || expired {
        c.disc(s / 2.0, s / 2.0, s / 2.0 - 0.5, RED);
        let text = badge_text(mentions, expired);
        let scale = if text.len() == 1 { (size / 8).max(1) } else { (size / 14).max(1) };
        c.text(&text, s / 2.0, s / 2.0, scale, [255, 255, 255]);
    } else if unread {
        c.disc(s / 2.0, s / 2.0, s * 0.3, RED);
    } else {
        return None;
    }
    Some(c.px)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_have_the_right_size() {
        assert_eq!(app_icon(32).len(), 32 * 32 * 4);
        assert_eq!(tray_icon(32, 3, true).len(), 32 * 32 * 4);
        assert_eq!(overlay_badge(16, 5, true, false).unwrap().len(), 16 * 16 * 4);
    }

    #[test]
    fn badge_states() {
        assert!(overlay_badge(16, 0, false, false).is_none());
        assert!(overlay_badge(16, 0, true, false).is_some());
        assert_eq!(badge_text(7, false), "7");
        assert_eq!(badge_text(150, false), "99+");
        assert_eq!(badge_text(0, true), "!");
    }

    #[test]
    fn app_icon_is_opaque_in_the_middle_and_transparent_in_corners() {
        let px = app_icon(64);
        let alpha = |x: usize, y: usize| px[(y * 64 + x) * 4 + 3];
        assert_eq!(alpha(32, 10), 255);
        assert_eq!(alpha(0, 0), 0);
    }
}

/// `cargo test export_icones -- --ignored` : régénère les PNG de `assets/icons`
/// utilisés par les installateurs (même dessin que l'icône de l'application).
#[cfg(test)]
mod export {
    #[test]
    #[ignore]
    fn export_icones() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("icons");
        std::fs::create_dir_all(&dir).unwrap();
        for size in [32u32, 64, 128, 256, 512] {
            let rgba = super::app_icon(size);
            let img = image::RgbaImage::from_raw(size, size, rgba).unwrap();
            img.save(dir.join(format!("{size}x{size}.png"))).unwrap();
        }
    }
}
