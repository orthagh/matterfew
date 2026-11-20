# Ce que nous apprennent les clients Mattermost officiels

Analyse du code source des trois clients officiels, pour guider la suite de
mm-gpui. Versions lues : `mattermost/mattermost` (client web, `webapp/`) du
20/11/2026, `mattermost/desktop` du 19/11/2026, `mattermost/mattermost-mobile`
du 20/11/2026.

## En bref

- **Le client de bureau officiel ne calcule presque rien lui-même** : c'est une
  coquille Electron autour du client web, qui lui transmet les compteurs et les
  notifications. Il apporte surtout l'intégration Windows (notifications avec
  identité, pastille de la barre des tâches, icône de notification, liens
  `mattermost://`, plusieurs serveurs).
- **Le client mobile est notre vrai modèle** : il n'embarque pas le client web,
  il parle directement à l'API comme nous. Sa force : une base locale (SQLite)
  qui permet un démarrage instantané et une synchronisation **incrémentale**
  (« ce qui a changé depuis ») au lieu de tout recharger.
- **Le client web est la référence des règles métier** : quand notifier,
  comment compter les non-lus, quoi recharger après une coupure.

## 1. Corrections à faire chez nous (robustesse)

Ces points sont des écarts de comportement, pas des fonctionnalités en plus.

| # | Problème actuel | Ce que font les officiels | Effort |
|---|---|---|---|
| 1 | **Les notifications ignorent les préférences** de l'utilisateur (niveau « tous / mentions / aucun », réglage par canal, ne pas déranger, absent). | Règle du web (`shouldSkipNotification`) : pas ses propres messages ; canal en sourdine → rien ; statut `dnd`/`ooo` → rien ; niveau du canal, sinon de l'utilisateur ; en fils réduits, une réponse ne notifie que si l'on suit le fil (`followers` dans l'événement `posted`) ; pas de notification pour le canal affiché si la fenêtre a le focus. | Faible |
| 2 | **Après une coupure**, on recharge seulement les 60 derniers messages du canal affiché : les éditions/suppressions plus anciennes et les non-lus des autres canaux peuvent être faux. | WebSocket « fiable » : `connection_id` + `sequence_number` dans l'URL ; si le serveur reprend la session, il rejoue les événements manqués. Sinon, rechargement ciblé : `GET /channels/{id}/posts?since=…` (renvoie nouveaux, modifiés et supprimés), membres de canaux, catégories, `GET /users/me/teams/unread`, `POST /users/ids?since=`. | Moyen |
| 3 | **Fils réduits supposés toujours actifs.** Si l'utilisateur les a désactivés, les réponses n'apparaissent pas dans le canal. | Actif si la config serveur `CollapsedThreads` ≠ `disabled` et (préférence `display_settings/collapsed_reply_threads` = `on` ou config `always_on`). Sinon : compteurs `total_msg_count` / `msg_count` / `mention_count` et réponses affichées dans le canal. | Faible |
| 4 | **`POST /users/ids` sans découpage** : au-delà de 100 utilisateurs (beaucoup de messages directs), l'appel échoue. Idem statuts au-delà de 200. | Lots de 100 (profils) et 200 (statuts), regroupés avec un délai de 100–200 ms. | Faible |
| 5 | Marquer lu : il manque `collapsed_threads_supported: true`. | `POST /channels/members/me/view {channel_id, collapsed_threads_supported: true}`. | Très faible |
| 6 | Statuts : seulement à l'arrivée d'un utilisateur, puis via `status_change`. | Rafraîchissement périodique (toutes les 60 s sur mobile) des statuts visibles. | Faible |
| 7 | Session expirée (HTTP 401) : simple message d'erreur. | État « session expirée » visible (pastille « ! » sur bureau) et retour à la connexion. | Faible |

## 2. Intégration Windows (ce que le client de bureau officiel ajoute)

Par ordre d'utilité :

1. **Identité des notifications** : `AppUserModelID` (« Mattermost.Desktop »
   chez eux) + raccourci du menu Démarrer portant cet identifiant. Sans cela,
   les notifications sont anonymes et **le clic ne peut pas ouvrir le bon
   canal**. Une seule notification gardée par canal ; clignotement de la barre
   des tâches ; sons personnalisés ; respect de « Assistant de concentration ».
2. **Pastille sur l'icône de la barre des tâches** (nombre de mentions, « • »
   pour des non-lus, « ! » si la session a expiré) et **icône dans la zone de
   notification** avec réduction dans cette zone à la fermeture.
3. **Instance unique + liens `mattermost://`** (enregistrés dans le registre
   utilisateur) : ouvrir un lien de message (permalien) dans l'application.
4. **Mémoriser la position et la taille de la fenêtre**, **démarrage avec
   Windows** (option « masqué »).
5. **Plusieurs serveurs** avec compteurs par serveur et raccourcis Ctrl+1…9.
6. Entreprise : confiance dans un certificat non reconnu (avec mémorisation),
   certificat client, authentification de proxy.
7. **Mises à jour** : le client officiel se contente de vérifier une version
   en ligne et d'ouvrir le lien de l'installeur.

Note sur la connexion SSO : le client officiel ouvre le fournisseur d'identité
dans le navigateur du système et revient par un lien `mattermost://`. Notre
fenêtre WebView2 fonctionne déjà avec GitLab ; on peut garder cette approche.

## 3. Fonctionnalités manquantes

### Peu d'effort

| Fonctionnalité | Comment |
|---|---|
| Ligne « Nouveaux messages » | Avant le premier message dont `create_at > last_viewed_at` (valeur prise à l'ouverture du canal), hors ses propres messages. Ouvrir à cet endroit : `GET /users/{uid}/channels/{cid}/posts/unread?limit_before=30&limit_after=30`. |
| Marquer comme non lu | `POST /users/{uid}/posts/{postId}/set_unread {collapsed_threads_supported: true}` ; ne plus marquer lu automatiquement tant qu'on ne quitte pas le canal. |
| Aperçus de liens | Déjà fournis par le serveur dans `post.metadata.embeds` (`opengraph`, `image`, `permalink`) : aucune requête en plus. |
| Pièces jointes des bots et intégrations (GitLab, etc.) | `post.props.attachments[]` : couleur, titre, texte, champs, image, pied ; boutons via `POST /posts/{id}/actions/{actionId}`. Aussi `props.override_username` / `override_icon_url`. |
| Messages épinglés, enregistrés, mentions récentes | `GET /channels/{id}/pinned`, `POST /posts/{id}/pin`/`unpin` ; préférence `flagged_post` + `GET /users/{uid}/posts/flagged`. |
| Historique des modifications | `GET /posts/{id}/edit_history` (absent du mobile). |
| Statut personnalisé | `PUT /users/me/status/custom {emoji, text, duration, expires_at}` ; lecture dans `user.props.customStatus`. |

### Effort moyen

| Fonctionnalité | Comment |
|---|---|
| Autocomplétion | `@` : `GET /users/autocomplete?in_team&in_channel&name` ; `~` : `GET /teams/{tid}/channels/autocomplete?name=` ; `:emoji:` : `GET /emoji/autocomplete?name=` ; `/` : `GET /teams/{tid}/commands/autocomplete_suggestions`. |
| Commandes slash | `POST /commands/execute {command, channel_id, team_id, root_id}`. |
| Emojis personnalisés | `post.metadata.emojis` liste ceux d'un message ; image `GET /emoji/{id}/image` ; `POST /emoji/names` (200 max). |
| Liens permanents | `/{team}/pl/{id}` ; `GET /posts/{id}` puis messages autour (`before`/`after` + `/thread`). |
| Vue « Fils de discussion » | `GET /users/{uid}/teams/{tid}/threads?extended=true` (+ `unread`, `since`), lire/suivre : `PUT …/threads/{id}/read/{ts}`, `PUT`/`DELETE …/following` ; événements `thread_updated`, `thread_read_changed`, `thread_follow_changed`. |
| Brouillons synchronisés | Si config `AllowSyncedDrafts` : `GET /users/me/teams/{tid}/drafts`, `POST /drafts`, `DELETE /users/me/channels/{cid}/drafts[/{rootId}]` (en-tête `Connection-Id`). Le mobile les garde seulement en local. |
| Recherche | Messages et fichiers, avec choix de l'équipe. |
| Parcourir / créer / rejoindre des canaux, ouvrir un message direct, profil d'un utilisateur | API classiques des canaux et utilisateurs. |

### Gros chantiers

- **Cache local (SQLite) et synchronisation incrémentale**, sur le modèle du
  mobile : affichage immédiat au lancement, puis mise à jour de « ce qui a
  changé depuis la dernière synchro » (`since`), messages téléchargés en
  priorité pour les canaux non lus.
- **Envoi optimiste** : le message s'affiche tout de suite avec un identifiant
  provisoire (`pending_post_id`), passe en « échec » avec bouton Réessayer si
  l'envoi rate ; l'écho du WebSocket est ignoré s'il est déjà affiché.
- Priorité des messages et accusés de lecture, messages programmés,
  dialogues interactifs, appels, playbooks.

## 4. Proposition d'ordre de travail

1. **Corrections 1 à 5** (notifications fidèles aux préférences, reprise
   après coupure, fils réduits, lots d'utilisateurs, marquer lu) : le client
   devient fiable au quotidien.
2. **Notifications Windows complètes** (identité, clic qui ouvre le canal,
   pastille de la barre des tâches, icône de la zone de notification).
3. **Ligne « Nouveaux messages », pièces jointes des intégrations, aperçus de
   liens** : très visibles, peu coûteux.
4. **Autocomplétion et emojis personnalisés.**
5. **Cache local et synchronisation incrémentale.**
