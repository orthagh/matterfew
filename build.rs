//! Icône de l'exécutable Windows (barre des tâches, explorateur, raccourci).
//!
//! L'icône est dessinée par le même code que celle de la zone de notification
//! (`src/icons.rs`), convertie ici en `.ico` puis intégrée comme ressource
//! n° 1 : c'est celle que GPUI charge pour ses fenêtres.

#[allow(dead_code, clippy::duplicated_attributes)]
#[path = "src/icons.rs"]
mod icons;

fn main() {
    println!("cargo:rerun-if-changed=src/icons.rs");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("app.ico");
    std::fs::write(&out, ico(&[16, 20, 24, 32, 40, 48, 64, 256])).expect("écriture de app.ico");

    let mut res = winresource::WindowsResource::new();
    res.set_icon_with_id(out.to_str().unwrap(), "1");
    res.set("ProductName", "Matterfew");
    res.set("FileDescription", "Matterfew");
    res.compile().expect("compilation des ressources Windows");
}

/// Fichier .ico (images BMP 32 bits avec transparence), une image par taille.
fn ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| bmp_entry(s, &icons::app_icon(s))).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&s, data) in sizes.iter().zip(&images) {
        let side = if s >= 256 { 0 } else { s as u8 };
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // plans
        out.extend_from_slice(&32u16.to_le_bytes()); // bits par pixel
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for data in images {
        out.extend_from_slice(&data);
    }
    out
}

/// Image d'icône au format DIB : en-tête, pixels BGRA de bas en haut, masque vide.
fn bmp_entry(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    for v in [40u32, size, size * 2] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    let image_bytes = size * size * 4 + mask_row * size;
    for v in [0u32, image_bytes, 0, 0, 0, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    out.extend(std::iter::repeat_n(0u8, (mask_row * size) as usize));
    out
}
