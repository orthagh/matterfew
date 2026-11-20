# mm-gpui — client Mattermost expérimental en Rust + GPUI

Preuve de concept d'un client de bureau natif pour Mattermost, sans Electron.
Interface : [GPUI](https://gpui.rs) (le framework de Zed, choisi par Discord pour
sa réécriture) via [GPUI Kit](https://gpui-kit.com) pour les composants.

## Ce qui fonctionne

| Fonction | Détail |
|---|---|
| Connexion | Via la page de connexion du serveur (GitLab, mot de passe, double authentification), par e-mail et mot de passe, ou par jeton ; mémorisée dans le gestionnaire d'identifiants Windows, bouton « Déconnexion » pour l'oublier. Dernière équipe et dernier canal de chaque équipe rouverts au lancement |
| Barre latérale | Chaque canal précédé d'une pastille colorée à ses initiales (« Dev backend » → DB), couleur stable tirée de son nom, petit cadenas pour les canaux privés. Catégories de l'utilisateur récupérées depuis l'API (Favoris, Canaux, Messages directs, catégories personnelles) dans son ordre, tri propre à chaque catégorie (alphabétique, récents, manuel), section « Non lus » si activée dans les préférences, limite de messages directs visibles, catégories repliables (synchronisé avec le serveur), canaux en sourdine atténués, mises à jour en temps réel |
| Messages | Historique infini (remonter charge la page précédente) avec bouton flottant « Revenir au présent », ligne « Nouveaux messages » à l'ouverture d'un canal non lu, regroupement par auteur, vos propres messages repérés par un liseré et un fond léger, @mentions en pastilles (les vôtres surlignées), `#étiquettes` cliquables (lancent la recherche), avatars et présence (en ligne, absent, ne pas déranger), messages qui vous mentionnent surlignés, « (modifié) », messages système |
| Intégrations et liens | Messages des intégrations (GitLab, webhooks…) en cartes : barre de couleur, auteur, titre cliquable, texte, champs, image, pied, boutons interactifs ; étiquette « BOT ». Aperçus de liens (site, titre, description, image), images liées, citations de messages. Les images externes sont chargées sans jamais envoyer le jeton |
| Envoi | Entrée pour envoyer, Maj+Entrée pour aller à la ligne, « X est en train d'écrire… » dans les deux sens. Autocomplétion : `@` personnes, `~` canaux, `:` emojis (standard et personnalisés), `/` commandes ; ↑ ↓ pour choisir, Entrée ou Tab pour insérer, Échap pour fermer. Les commandes « / » sont exécutées par le serveur |
| Actions sur un message | Au survol : réactions rapides, Répondre et « ⋯ » : Marquer comme non lu, Enregistrer, Épingler au canal, Copier le lien, Copier le texte, et pour ses propres messages Modifier / Supprimer (avec confirmation). Étiquettes « Épinglé » et « Enregistré ». Cliquer une réaction existante l'ajoute ou la retire |
| Options des canaux | En-tête : étoile de favori à côté du nom (cloche barrée si en sourdine), puis recherche, mentions, fils, fichiers, membres et « ⋯ » (messages épinglés, enregistrés, favori, sourdine, copier le lien, quitter). Clic droit sur un canal de la barre latérale : marquer comme lu, favori, sourdine, copier le lien, quitter. « ⋯ » d'une catégorie : tri (alphabétique, activité récente, manuel) et « marquer la catégorie comme lue ». Menu du compte : statut (en ligne, absent, ne pas déranger, hors ligne) et déconnexion |
| Éditeur | Zone de saisie aérée : bouton « Aa » pour afficher ou masquer la barre de mise en forme (réglage mémorisé), emoji, pièce jointe, aperçu, bouton d'envoi (grisé tant qu'il n'y a rien à envoyer). Mise en forme : gras, italique, barré, titre, lien, code, bloc de code, citation, listes), raccourcis Ctrl+B, Ctrl+I, Ctrl+Maj+X, Ctrl+Alt+K, Ctrl+Alt+C, aperçu du rendu. Brouillon conservé par canal et par fil (crayon dans la barre latérale) |
| Fils | Compteur « N réponses », panneau latéral avec réponse dans le fil |
| Navigation | Ctrl+K : sélecteur de canaux (recherche sans accents, ↑ ↓ Entrée, canaux récents si rien n'est tapé). Panneau de droite : messages épinglés du canal, messages enregistrés ; un clic ouvre le canal et le fil du message |
| Recherche | Champ permanent dans l'en-tête (Ctrl+F). Pendant la saisie : filtres en pastilles (`from:`, `in:`, `on:`, `after:`, `before:`, `-mot`), suggestions toutes prêtes (dans ce canal, mes messages, messages de la personne en message direct, aujourd'hui, 7 derniers jours), étiquettes du canal, recherches récentes (8, effaçables) ; `from:` et `in:` proposent les personnes et les canaux au fil de la frappe. Résultats dans le panneau de droite, la requête affichée en jetons qu'un clic sur × retire |
| Fichiers du canal | Icône dossier dans l'en-tête : onglets Médias (galerie par mois, lignes justifiées qui gardent les proportions des images, nom au survol, visionneuse), Fichiers (ouvrir, télécharger) et Liens (adresses des messages chargés, avec le titre de leur aperçu) |
| Membres | Liste permanente des membres du canal à droite : administrateurs (système ou du canal) d'abord, puis par statut (en ligne, absent, ne pas déranger, hors ligne), comme Discord ; chaque groupe a sa couleur (violet pour les administrateurs, dont le nom prend aussi la couleur, vert, orange, rouge, gris) ; un clic ouvre la carte de profil. Dans un message direct, la carte de la personne remplace la liste. Masquable depuis l'en-tête (réglage mémorisé) ; s'efface quand un fil ou un panneau est ouvert |
| Fils suivis | Icône dans l'en-tête : fils suivis du canal affiché, avec participants (avatars empilés), nombre de réponses et réponses non lues ; ouvrir un fil le marque comme lu |
| Mentions récentes | Icône @ dans l'en-tête : messages qui vous mentionnent, toutes équipes confondues ; un clic ouvre l'équipe, le canal et le fil |
| Carte de profil | Un clic sur une personne (avatar ou nom d'un message, @mention, liste des membres, participants d'un fil) ouvre une mini-carte : photo en médaillon avec son statut, nom, poste, adresse e-mail, heure locale et écart avec la vôtre, rôle d'administrateur, bouton « Envoyer un message ». Échap ou un clic à côté la ferme |
| Emojis | Sélecteur d'emojis (catégories, recherche, emojis du serveur) pour réagir à un message ou insérer dans la saisie |
| Fichiers | Aperçu des images jointes (miniature ou fichier complet si le serveur n'a pas d'aperçu, carte de fichier en dernier recours, sans message d'erreur) (clic : visionneuse intégrée, Échap pour fermer), cartes pour les autres fichiers avec boutons icônes Ouvrir et Télécharger (dossier Téléchargements) ; envoi par le bouton « Joindre » ou en glissant des fichiers dans la fenêtre |
| Équipes | Barre des équipes avec leur logo ; pilule au bord gauche pour l'équipe affichée (courte pour une équipe avec des non-lus), pastille de non-lus et compteur de mentions |
| Mise en page | Barre latérale redimensionnable en glissant son bord droit (largeur mémorisée dans `%APPDATA%\mm-gpui\ui.json`) ; barres de défilement visibles seulement pendant le défilement |
| Cache local | Derniers messages de chaque canal (60) et leurs auteurs gardés dans `%LOCALAPPDATA%\mm-gpui\cache\<serveur>` : un canal déjà ouvert s'affiche instantanément, puis la réponse du serveur remplace l'affichage. 40 Mo au plus (les canaux les plus anciens sont retirés), effacé à la déconnexion |
| Temps réel | WebSocket « fiable » : en cas de coupure, le serveur rejoue les événements manqués ; s'il ne le peut plus, rattrapage ciblé (canaux, non-lus, messages modifiés depuis le dernier connu, fil ouvert). Présence rafraîchie chaque minute |
| Notifications | Notification Windows selon vos préférences Mattermost, comme le client web : niveau « tous / mentions / aucun » de l'utilisateur ou du canal, canaux en sourdine, « Ne pas déranger », fils suivis ; rien pour ce qui est déjà affiché. Au nom de l'application, une par canal ; un clic ouvre le canal ; le bouton de la barre des tâches clignote |
| Intégration Windows | Icône de l'application intégrée à l'exécutable (fenêtre, barre des tâches, explorateur, menu Démarrer), dessinée par le code et convertie à la compilation (`build.rs`). Pastille sur l'icône de la barre des tâches (nombre de mentions, point pour les non-lus, « ! » si la session a expiré), icône dans la zone de notification (état, menu Ouvrir / Quitter) ; fermer la fenêtre la réduit dans cette zone ; une seule instance (relancer ramène la fenêtre) |
| Markdown | Rendu complet (gras, italique, listes, code, tableaux, citations, liens cliquables), texte sélectionnable |
| Emojis | `:smile:` → 😄 dans le texte et les réactions ; emojis personnalisés du serveur affichés en image |

Non couvert : coller une image depuis le presse-papiers,
appels, plugins, réorganisation des canaux par glisser-déposer.

> **Au premier lancement sous Windows**, l'application crée un raccourci
> « Mattermost (mm-gpui) » dans le menu Démarrer et une entrée
> `HKCU\Software\Classes\AppUserModelId\MmGpui.Client` : Windows n'affiche
> les notifications d'une application non installée qu'à cette condition.
> Les supprimer suffit pour tout retirer.

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

### Compilation

Seul le code modifié est recompilé ; les bibliothèques (GPUI…) ne le sont
qu'une fois.

| Commande | Usage | Recompilation après une modification |
|---|---|---|
| `cargo run` | développement (bibliothèques optimisées, notre code peu) | ~7 s |
| `cargo run --release` | usage quotidien | ~6 s |
| `cargo build --profile dist` | binaire à distribuer (optimisation maximale) | ~1 min |

### Options

| Option | Effet |
|---|---|
| `--demo` | Faux serveur avec des données fictives (aucune connexion réseau) |
| `--login` | Affiche l'écran de connexion sans utiliser la connexion mémorisée |
| `--demo --theme fichier.json` | Mode démo avec un thème Mattermost (JSON obtenu par « Copier le thème » dans Paramètres › Affichage › Thème) |

```bash
cargo run --release -- --demo
```

### Apparence

Les couleurs viennent du thème Mattermost de l'utilisateur (Profil ›
Paramètres › Affichage › Thème), lu dans ses préférences : thème de l'équipe
s'il y en a un, sinon thème global, sinon « Denim ».

Les dégradés (écran de connexion, barre latérale, barre des équipes, en-tête,
carte de profil) sont calculés à partir de ces mêmes couleurs : ils suivent
donc le thème. Menu du compte › Apparence › Dégradés pour les désactiver.

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
├── notify_rules.rs  quand afficher une notification — testé
├── desktop.rs   intégration Windows (notifications, pastille, zone de notification)
├── icons.rs     icônes dessinées par le code (application, pastilles) — testé
├── rich.rs      intégrations et aperçus de liens — testé
├── editor.rs    mise en forme Markdown de la zone de saisie — testé
├── fuzzy.rs     recherche rapide de canaux (Ctrl+K) — testé
├── settings.rs  préférences locales (largeur, dernière équipe et canaux)
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
