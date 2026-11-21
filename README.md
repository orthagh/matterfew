# mm-gpui

Un client de bureau pour [Mattermost](https://mattermost.com), natif et léger :
écrit en Rust avec [GPUI](https://gpui.rs), le moteur d'interface de Zed. Pas
d'Electron, pas de navigateur embarqué.

![Le client, sur un serveur de démonstration](docs/capture.png)

## Ce qu'il sait faire

- Tout le quotidien : canaux et catégories de votre serveur, fils, réactions,
  pièces jointes, recherche, mentions, messages directs.
- Une interface rapide qui reprend le thème de votre Mattermost.
- Les notifications des bots (GitHub, GitLab…) condensées sur une ligne.
- Notifications et pastilles du système, connexion mémorisée dans le
  trousseau.

Le détail est dans [docs/FONCTIONNALITES.md](docs/FONCTIONNALITES.md).

## Installer

Téléchargez la dernière version dans les
[Releases](https://github.com/orthagh/mattermost_rust_client/releases) :
installateur Windows, image disque macOS, snap ou flatpak pour Linux.

Les versions ne sont pas encore signées : Windows et macOS demandent une
confirmation au premier lancement.

## Essayer sans serveur

```bash
cargo run -- --demo
```

## Compiler

Il faut [Rust](https://rustup.rs) (stable). Sous Linux, quelques
bibliothèques système en plus : voir `packaging/linux-deps.sh`.

```bash
cargo build --release
cargo test
```

Les évolutions de chaque version sont dans le [CHANGELOG](CHANGELOG.md).

## Licence

[MIT](LICENSE)
