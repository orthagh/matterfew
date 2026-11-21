# Journal des modifications

Les évolutions notables de chaque version. Format :
[Keep a Changelog](https://keepachangelog.com/fr/1.1.0/), versions :
[SemVer](https://semver.org/lang/fr/).

Pour publier une version : déplacer le contenu de « Non publié » sous un
titre `## [x.y.z] - AAAA-MM-JJ`, mettre la même version dans `Cargo.toml`,
puis pousser le tag `vx.y.z`. La release GitHub reprend cette section.

## [Non publié]

### Ajouté

- Connexion par la page du serveur (GitLab, mot de passe, double
  authentification), par e-mail ou par jeton ; mémorisée dans le trousseau
  du système.
- Barre latérale fidèle à Mattermost : catégories, tri, non-lus, favoris,
  sourdine (aussi par catégorie), réorganisation par glisser-déposer,
  pastilles colorées des canaux ; création de canaux et de catégories,
  invitations.
- Messages : Markdown, emojis Twemoji, réactions, fils, épinglés, enregistrés,
  pièces jointes, aperçus de liens, intégrations, notifications de bots
  regroupées sur une ligne, liens automatiques (plugin Autolink).
- Recherche avec filtres proposés, étiquettes et historique ; mentions
  récentes ; fichiers, médias et liens du canal ; sélecteur Ctrl+K.
- Cartes de profil, d'équipe et panneau « À propos » du canal ; liste des
  membres par rôle et statut.
- Éditeur : mise en forme, aperçu, autocomplétion (personnes, canaux, emojis,
  commandes « / » et leurs paramètres), correction orthographique (Windows).
- Thème du serveur, dégradés, mise en page adaptée aux fenêtres étroites.
- Windows : notifications, pastille de la barre des tâches, zone de
  notification, une seule instance.
- Cache local des derniers messages ; WebSocket fiable avec rattrapage.
- Interface en anglais, français et portugais du Brésil (catalogues
  `locales/*.yml`), langue du système détectée, `MM_LANG` pour la forcer.
- CI (format, Clippy, tests sur Windows, Linux, macOS, audit des dépendances)
  et installateurs sur tag (Windows, macOS, Linux snap et flatpak).
