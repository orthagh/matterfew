"""Manifeste de mise à jour (latest.json) lu par l'application.

    python packaging/updater-manifest.py ARTEFACTS VERSION NOTES.md DÉPÔT SORTIE

ARTEFACTS : dossier des artefacts de la release (sous-dossiers packages-*).
Chaque plateforme pointe vers son paquet et la signature minisign produite
par `cargo packager` (fichier .sig voisin). Sans signature (pas de clé dans
les secrets), rien n'est écrit : l'application ne propose alors que l'avis.
"""

import datetime
import json
import pathlib
import sys

# Plateforme du manifeste → (motif du paquet, format pour le moteur).
PLATFORMS = {
    "windows-x86_64": ("*_x64-setup.exe", "nsis"),
    "macos-aarch64": ("*_aarch64.app.tar.gz", "app"),
    "macos-x86_64": ("*_x64.app.tar.gz", "app"),
    "linux-x86_64": ("*.AppImage", "appimage"),
}


def main(artifacts, version, notes_path, repo, out):
    root = pathlib.Path(artifacts)
    notes = pathlib.Path(notes_path).read_text(encoding="utf-8").strip() if pathlib.Path(notes_path).exists() else ""
    platforms = {}
    missing = []
    for key, (pattern, fmt) in PLATFORMS.items():
        packages = sorted(root.glob(f"packages-*/{pattern}"))
        if len(packages) != 1:
            missing.append(f"{key}: {len(packages)} paquet(s) pour {pattern}")
            continue
        package = packages[0]
        sig = package.with_name(package.name + ".sig")
        if not sig.exists():
            missing.append(f"{key}: pas de {sig.name}")
            continue
        platforms[key] = {
            "url": f"https://github.com/{repo}/releases/download/v{version}/{package.name}",
            "signature": sig.read_text(encoding="utf-8").strip(),
            "format": fmt,
        }
    if missing:
        for line in missing:
            print(f"::warning::latest.json non écrit : {line}")
        return 0
    manifest = {
        "version": version,
        "notes": notes,
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
    }
    pathlib.Path(out).parent.mkdir(parents=True, exist_ok=True)
    pathlib.Path(out).write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps({k: v["url"] for k, v in platforms.items()}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:6]))
