//! Sons de notification, synthétisés ici (aucun fichier audio embarqué).
//! Les noms reprennent ceux du client web (`desktop_notification_sound`)
//! pour que le choix reste le même d'un client à l'autre.

/// Sons proposés, dans l'ordre du client web.
pub const SOUNDS: [&str; 6] = ["Bing", "Crackle", "Down", "Hello", "Ripple", "Upstairs"];

/// Son par défaut du serveur.
pub const DEFAULT: &str = "Bing";

const RATE: u32 = 22_050;

/// Note : fréquence (Hz), début et durée (s).
struct Note(f32, f32, f32);

/// Échantillons (‑1..1) d'un son.
fn samples(name: &str) -> Vec<f32> {
    match name {
        "Crackle" => crackle(),
        "Down" => notes(&[Note(1318.5, 0., 0.45), Note(880., 0.14, 0.6)], 0.6),
        "Hello" => notes(&[Note(784., 0., 0.4), Note(1174.7, 0.13, 0.65)], 0.6),
        "Ripple" => notes(
            &[Note(1046.5, 0., 0.3), Note(1318.5, 0.06, 0.3), Note(1568., 0.12, 0.3), Note(2093., 0.18, 0.5)],
            0.75,
        ),
        "Upstairs" => notes(&[Note(523.3, 0., 0.35), Note(659.3, 0.12, 0.35), Note(784., 0.24, 0.6)], 0.5),
        _ => bell(1046.5, 0.9),
    }
}

/// Cloche : fondamentale et partiels inharmoniques, décroissance rapide.
fn bell(freq: f32, len: f32) -> Vec<f32> {
    let n = (len * RATE as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let tone = |mult: f32, amp: f32, decay: f32| {
                amp * (std::f32::consts::TAU * freq * mult * t).sin() * (-t * decay).exp()
            };
            attack(t) * (tone(1., 1., 5.) + tone(2.76, 0.35, 9.) + tone(5.4, 0.12, 14.))
        })
        .collect()
}

/// Notes successives au timbre de marimba (fondamentale + octave brève).
fn notes(list: &[Note], total: f32) -> Vec<f32> {
    let n = (total.max(list.iter().map(|Note(_, at, len)| at + len).fold(0., f32::max)) * RATE as f32) as usize;
    let mut out = vec![0.; n];
    for Note(freq, at, len) in list {
        let start = (at * RATE as f32) as usize;
        let count = (len * RATE as f32) as usize;
        for i in 0..count.min(n.saturating_sub(start)) {
            let t = i as f32 / RATE as f32;
            let w = std::f32::consts::TAU * freq * t;
            out[start + i] += attack(t) * (-t * 7.).exp() * (w.sin() + 0.25 * (2. * w).sin() * (-t * 20.).exp());
        }
    }
    out
}

/// Petites impulsions de bruit filtré, à intervalles irréguliers.
fn crackle() -> Vec<f32> {
    let n = (0.45 * RATE as f32) as usize;
    let mut out = vec![0.; n];
    let mut seed: u32 = 0x2f6b_9a31;
    let mut rand = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };
    for (k, at) in [0.0, 0.05, 0.08, 0.15, 0.19, 0.27].iter().enumerate() {
        let start = (at * RATE as f32) as usize;
        let mut prev = 0.;
        for i in 0..(0.05 * RATE as f32) as usize {
            let t = i as f32 / RATE as f32;
            // Passe-bas d'ordre un : un grésillement plutôt qu'un souffle.
            prev = 0.55 * prev + 0.45 * (rand() * 2. - 1.);
            if let Some(s) = out.get_mut(start + i) {
                *s += prev * (-t * 90.).exp() * (1. - k as f32 * 0.1);
            }
        }
    }
    out
}

fn attack(t: f32) -> f32 {
    (t / 0.004).min(1.)
}

/// Fichier WAV (PCM 16 bits mono) du son, normalisé.
pub fn wav(name: &str) -> Vec<u8> {
    let raw = samples(name);
    let peak = raw.iter().fold(0f32, |m, s| m.max(s.abs())).max(1e-6);
    let data: Vec<u8> = raw.iter().flat_map(|s| ((s / peak * 0.55 * i16::MAX as f32) as i16).to_le_bytes()).collect();
    let mut out = Vec::with_capacity(44 + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// Nom connu (sinon le son par défaut).
pub fn normalize(name: &str) -> &'static str {
    SOUNDS.iter().find(|s| s.eq_ignore_ascii_case(name)).copied().unwrap_or(DEFAULT)
}

/// Joue un son sans attendre la fin.
#[cfg(windows)]
pub fn play(name: &str) {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    use windows::core::PCWSTR;

    // Lecture asynchrone depuis la mémoire : le tampon doit vivre jusqu'à la
    // fin du son, on garde donc chacun des six sons une fois créé.
    static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static [u8]>>> = OnceLock::new();
    let name = normalize(name);
    let bytes = *CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(name)
        .or_insert_with(|| Box::leak(wav(name).into_boxed_slice()));
    unsafe {
        let _ = PlaySoundW(PCWSTR(bytes.as_ptr().cast()), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

/// Joue un son sans attendre la fin (`afplay` sous macOS, PulseAudio ou ALSA sous Linux).
#[cfg(not(windows))]
pub fn play(name: &str) {
    let name = normalize(name);
    let Some(dir) = dirs::cache_dir().map(|d| d.join("matterfew").join("sounds")) else { return };
    let path = dir.join(format!("{name}.wav"));
    if !path.exists() {
        let _ = std::fs::create_dir_all(&dir);
        if std::fs::write(&path, wav(name)).is_err() {
            return;
        }
    }
    let players: &[(&str, &[&str])] =
        if cfg!(target_os = "macos") { &[("afplay", &[])] } else { &[("paplay", &[]), ("aplay", &["-q"])] };
    std::thread::spawn(move || {
        for (cmd, args) in players {
            if let Ok(mut child) = std::process::Command::new(cmd).args(*args).arg(&path).spawn() {
                let _ = child.wait();
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sons_valides() {
        for name in SOUNDS {
            let wav = wav(name);
            assert_eq!(&wav[..4], b"RIFF");
            assert!(wav.len() > 44 + 2 * RATE as usize / 4, "{name} trop court");
            assert!(wav.len() < 44 + 2 * RATE as usize * 2, "{name} trop long");
        }
        assert_eq!(normalize("upstairs"), "Upstairs");
        assert_eq!(normalize("inconnu"), DEFAULT);
    }
}
