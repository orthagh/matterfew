# Journal des modifications

Les évolutions notables de chaque version. Format :
[Keep a Changelog](https://keepachangelog.com/fr/1.1.0/), versions :
[SemVer](https://semver.org/lang/fr/).

Pour publier une version : déplacer le contenu de « Non publié » sous un
titre `## [x.y.z] - AAAA-MM-JJ`, mettre la même version dans `Cargo.toml`,
puis pousser le tag `vx.y.z`. La release GitHub reprend cette section.

## [Non publié]

### Ajouté

- Fenêtre « Paramètres » (menu du compte, Ctrl+Maj+A), en quatre onglets :
  - Notifications : bureau (tous, mentions, rien), réponses dans les fils,
    e-mail, son choisi parmi six (synthétisés, mêmes noms que le client
    web), mots qui vous mentionnent, mots surlignés sans notifier,
    réponse automatique en message direct ;
  - Affichage : fils réduits, horloge 12 ou 24 heures, format des noms ;
  - Barre latérale : canaux non lus regroupés, nombre de messages directs ;
  - Avancé : envoi avec Ctrl+Entrée, barre de mise en forme.
- Mots-clés surlignés dans les messages, comme les mentions.
- « Ne pas déranger » avec une durée (30 min, 1 h, 2 h, jusqu'à demain
  9 h, ou date et heure choisies), dans le menu du compte ; l'échéance
  s'affiche sous votre nom.
- Filtre « Non lus seulement » dans la barre latérale (bouton à côté de
  « Aller à un canal », Ctrl+Maj+U).
- « Fermer la conversation » sur un message direct ou de groupe : il
  quitte la barre latérale et revient au prochain message.
- Équipes : rejoindre une autre équipe ouverte, quitter une équipe,
  paramètres de l'équipe (nom, description, ouverte à tous) depuis la
  carte d'équipe.
- Coloration syntaxique des blocs de code (environ 200 langages), aux
  couleurs du thème clair ou sombre.

## [0.2.0] - 2026-11-22

### Modifié

- Bouton « + » de la barre latérale plus discret ; couleurs de présence
  dans le menu des statuts ; pastille bleue pour les mentions des autres
  équipes ; entrée « Fils » visible dès que le serveur gère les fils suivis.

- L'application s'appelle désormais **Matterfew** (dépôt
  `orthagh/matterfew`). Réglages, cache et connexion enregistrés sous
  l'ancien nom sont repris automatiquement au premier lancement.

### Ajouté

- Liens vers ce serveur (message, canal, message direct) et `~canal`
  ouverts dans l'application plutôt que dans le navigateur.
- Coller une image ou des fichiers (Ctrl+V) les joint au message.
- Flèche ↑ dans une zone de saisie vide : modifier son dernier message.
- Alt+↑/↓ : canal précédent ou suivant ; Alt+Maj+↑/↓ : canal non lu.
- Suivre ou ne plus suivre un fil, depuis le menu d'un message.
- « Me le rappeler » : dans 30 min, 1 h, 2 h ou demain.
- Préférences de notification par canal : bureau (par défaut, tout,
  mentions, jamais), sourdine, ignorer @channel/@here/@all, suivre
  automatiquement les nouveaux fils. Synchronisées avec les autres appareils.
- « Parcourir les canaux » : canaux publics à rejoindre et canaux archivés,
  ouverts en lecture seule.
- Nouveau message direct ou de groupe (jusqu'à 7 personnes), depuis le
  bouton « + », la catégorie Messages directs ou Ctrl+Maj+K.
- Statut personnalisé : emoji et texte à côté des noms (messages, membres,
  messages directs, carte de profil) ; réglage avec suggestions et échéance.
- Vue « Fils » de toute l'équipe en haut de la barre latérale, avec sa
  pastille, l'onglet « Non lus » et « Tout marquer comme lu ».
- Recherche dans les fichiers et liens d'un canal.
- Transférer un message vers un canal ou une personne, avec un commentaire.
- Infobulle « qui a réagi » sur les réactions.
- Message non envoyé : réessayer, le modifier ou l'abandonner.
- Envoi avec Ctrl+Entrée si la préférence du serveur le demande (Entrée
  va alors à la ligne) ; dans un bloc de code, Entrée va à la ligne.
- Confirmation avant @channel, @all ou @here dans un canal de plus de
  5 personnes.
- En message direct : avertissement si la personne est en « Ne pas
  déranger », absente, ou s'il fait nuit chez elle.
- Mention d'une personne absente du canal : bouton pour l'y ajouter.
- Emojis récents : en tête du sélecteur et comme réactions rapides.
- Toasts : « N nouveaux messages » en bas, « Nouveaux messages depuis… »
  en haut de la liste.
- Paramètres du canal : nom, adresse, passage en privé, archivage et
  désarchivage.
- Membres : nommer ou retirer un administrateur, retirer du canal (clic droit).
- Catégories : renommer, supprimer (leurs canaux reviennent dans « Canaux »).
- Raccourcis : aide (Ctrl+/ ou F1), Maj+↑ répondre au dernier message,
  Maj+Échap tout marquer comme lu, Ctrl+Maj+M mentions, Ctrl+Maj+L zone de
  saisie, Alt+←/→ historique des canaux, Ctrl+Alt+↑/↓ équipe.
- Bandeau en haut de l'écran quand la connexion au serveur est perdue.
- Signets de canal : barre de liens et de fichiers sous l'en-tête, ajout
  d'un lien, suppression (clic droit), mise à jour en direct.
- Aperçu des PDF dans la visionneuse (Windows : pages rendues par le
  système ; ailleurs, ouverture avec l'application par défaut).
- Accessibilité : boutons à icône nommés, canaux, équipes et messages lus
  par les lecteurs d'écran, zones de l'écran annoncées, F6 pour passer
  d'une zone de saisie à l'autre.

## [0.1.0] - 2026-11-21

Première version publique.

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
