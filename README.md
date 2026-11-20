# mm-gpui — client Mattermost expérimental en Rust + GPUI

Preuve de concept d'un client de bureau natif pour Mattermost, sans Electron.
Interface : [GPUI](https://gpui.rs) (le framework de Zed, choisi par Discord pour
sa réécriture) via [GPUI Kit](https://gpui-kit.com) pour les composants.

## Ce qui fonctionne

| Fonction | Détail |
|---|---|
| Connexion | Jeton d'accès personnel, par formulaire ou variables d'environnement ; mémorisé dans le gestionnaire d'identifiants Windows, bouton « Déconnexion » pour l'oublier |
| Barre latérale | Catégories de l'utilisateur récupérées depuis l'API (Favoris, Canaux, Messages directs, catégories personnelles) dans son ordre, tri propre à chaque catégorie (alphabétique, récents, manuel), section « Non lus » si activée dans les préférences, limite de messages directs visibles, catégories repliables (synchronisé avec le serveur), canaux en sourdine atténués, mises à jour en temps réel |
| Messages | Historique infini (remonter charge la page précédente), regroupement par auteur, avatars et présence (en ligne, absent, ne pas déranger), « (modifié) », messages système |
| Envoi | Entrée pour envoyer, Maj+Entrée pour aller à la ligne, « X est en train d'écrire… » dans les deux sens |
| Actions sur un message | Au survol : réactions rapides, Répondre, et pour ses propres messages Modifier / Supprimer (avec confirmation) ; cliquer une réaction existante l'ajoute ou la retire |
| Fils | Compteur « N réponses », panneau latéral avec réponse dans le fil |
| Temps réel | WebSocket : messages, éditions, suppressions, réactions, présence, frappe, catégories, préférences ; reconnexion automatique |
| Notifications | Notification Windows pour les messages directs et les mentions (si la fenêtre n'a pas le focus ou si le canal n'est pas affiché) |
| Markdown | Rendu complet (gras, italique, listes, code, tableaux, citations, liens cliquables), texte sélectionnable |
| Emojis | `:smile:` → 😄 dans le texte et les réactions |

Non couvert : recherche, fichiers et images, autocomplétion, SSO, appels,
plugins, réorganisation des canaux par glisser-déposer.

## Lancer

Prérequis Windows : Rust (`rustup`) et Visual Studio Build Tools avec la charge
« Développement Desktop en C++ ».

```bash
cargo run --release
```

### Connexion

Trois façons de se connecter, au choix sur l'écran d'accueil :

1. **Se connecter via Mattermost** (recommandé) : une petite fenêtre de
   navigateur intégré (WebView2) ouvre la vraie page de connexion du serveur.
   GitLab, mot de passe et double authentification fonctionnent comme dans le
   navigateur. Le client récupère ensuite le cookie de session.
2. **E-mail + mot de passe** directement dans l'application (le code de double
   authentification est demandé si le compte en a une).
3. **Jeton d'accès personnel** (option « avancé »), ou variables
   d'environnement `MM_URL` + `MM_TOKEN`.

La connexion est mémorisée : jeton dans le Gestionnaire d'identifiants Windows,
adresse du serveur dans `%APPDATA%\mm-gpui\config.json`. « Déconnexion » oublie
le jeton ; pour une session (cas 1 et 2), elle est aussi fermée côté serveur.

### Options

| Option | Effet |
|---|---|
| `--demo` | Faux serveur avec des données fictives (aucune connexion réseau) |
| `--login` | Affiche l'écran de connexion sans utiliser la connexion mémorisée |

```bash
cargo run --release -- --demo
```

### Apparence

Les couleurs viennent du thème Mattermost de l'utilisateur (Profil ›
Paramètres › Affichage › Thème), lu dans ses préférences : thème de l'équipe
s'il y en a un, sinon thème global, sinon « Denim ».

> **Chemins longs sous Windows** : si le dossier du projet a un chemin très
> long, l'éditeur de liens échoue (`LNK1104`). Placez le projet dans un
> dossier court ou définissez `CARGO_TARGET_DIR` vers un dossier court.

## Organisation du code

```
src/
├── main.rs      démarrage de l'application et de la fenêtre
├── ui/          interface GPUI (connexion, canaux, messages, fil, saisie)
│   ├── mod.rs   état de l'application et réaction aux événements
│   └── view.rs  rendu des écrans
├── sidebar.rs   regroupement et tri des canaux (catégories, non lus) — testé
├── theme.rs     thème Mattermost de l'utilisateur → couleurs de l'interface
├── sso.rs       fenêtre de connexion « via Mattermost » (WebView2)
├── credentials.rs  mémorisation de la connexion
├── demo.rs      faux serveur pour le mode --demo
├── backend.rs   thread réseau tokio : commandes de l'UI → événements vers l'UI
├── api.rs       client REST Mattermost v4 (reqwest)
├── markdown.rs  préparation du Markdown (emojis, mentions)
├── emoji.rs     codes :emoji: → Unicode
└── notify.rs    notifications de bureau
```

L'interface et le réseau tournent sur des exécuteurs différents (GPUI d'un côté,
tokio de l'autre) et communiquent uniquement par deux canaux de messages : c'est
ce qui permet de garder l'interface fluide pendant les appels réseau.

```bash
cargo test   # tests du traitement Markdown et des emojis
```
