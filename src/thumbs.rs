//! Images reçues du serveur, préparées pour l'affichage.

/// Réduit une image (logo d'équipe…) à `max` pixels au plus, avec un filtre de
/// qualité : affichée petite, une grande image réduite par la carte graphique
/// paraît crénelée. Renvoie un PNG, ou `None` si l'image est illisible.
pub fn downscale_png(bytes: &[u8], max: u32) -> Option<Vec<u8>> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width() > max || img.height() > max {
        img.resize(max, max, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::downscale_png;

    #[test]
    fn reduction_des_logos() {
        let big = image::RgbaImage::from_pixel(600, 300, image::Rgba([10, 20, 30, 255]));
        let mut png = Vec::new();
        big.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let small = image::load_from_memory(&downscale_png(&png, 96).unwrap()).unwrap();
        assert_eq!((small.width(), small.height()), (96, 48));
        assert!(downscale_png(b"pas une image", 96).is_none());
    }
}
