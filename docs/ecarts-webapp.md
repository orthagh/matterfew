# Écarts avec le client web officiel

Comparaison fonctionnelle entre matterfew (voir [FONCTIONNALITES.md](FONCTIONNALITES.md))
et la webapp officielle ([mattermost/mattermost](https://github.com/mattermost/mattermost),
branche `master`, novembre 2026). Le design (couleurs, typographie, espacements,
Compass) est traité ailleurs : seuls les fonctions et comportements sont listés ici.

**Méthode.** Index des chaînes `webapp/channels/src/i18n/en.json` (≈ 9 000 clés),
liste des composants `webapp/channels/src/components/`, puis recherche dans notre
code (`src/ui/view.rs`, `src/ui/mod.rs`, `src/backend.rs`, `src/api.rs`, `locales/`)
avant de déclarer une fonction absente.

**Conventions.** Chemins relatifs à
[`webapp/channels/src/components/`](https://github.com/mattermost/mattermost/tree/master/webapp/channels/src/components)
(noté `C/`) ; clés i18n en `code`. Effort : **S** < 1 jour, **M** quelques jours,
**L** > 1 semaine. Valeur : ★ confort, ★★ utile au quotidien, ★★★ attendu par
tout utilisateur venant du client officiel. API REST : `/api/v4`.

---

## 1. Actions sur un message

| Fonction | Où dans la webapp | Comportement officiel | Chez nous | Effort | Valeur |
|---|---|---|---|---|---|
| ~~Rappel « Me le rappeler »~~ | ~~`C/dot_menu/post_reminder_submenu.tsx`, `C/post_reminder_custom_time_picker_modal`, `post_info.post_reminder.*`~~ | ~~Sous-menu 30 min / 1 h / 2 h / demain / personnalisé ; le bot système renvoie le message à l'heure dite (`POST /users/{id}/posts/{post}/reminder`)~~ | ~~Absent~~ | ~~S~~ | ~~★★★~~ |
| ~~Transférer un message~~ | ~~`C/forward_post_modal`, `forward_post_modal.*`~~ | ~~Choix d'un canal ou d'une personne, commentaire facultatif, aperçu ; avertit si la source est privée~~ | ~~Absent~~ | ~~M~~ | ~~★★~~ |
| ~~Suivre / ne plus suivre le fil~~ | ~~`threading.threadMenu.follow*`, `C/threading/common`~~ | ~~Bouton dans l'en-tête du fil et menu « ⋯ » (`PUT/DELETE /users/{u}/teams/{t}/threads/{id}/following`)~~ | ~~Absent (le backend connaît seulement l'état « suivi » pour les notifications)~~ | ~~S~~ | ~~★★★~~ |
| Historique des modifications | `C/post_edit_history`, `post_info.edit.*`, `search_header.title_edit.history` | Clic sur « (modifié) » : versions précédentes dans le panneau droit, restauration possible (`GET /posts/{id}/edit_history`) | Partiel : libellé « (modifié) » seulement | M | ★★ |
| Priorité Important / Urgent | `C/post_priority`, `C/advanced_text_editor/priority_labels`, `post_priority.*` | Sélecteur dans la barre de mise en forme ; étiquette colorée sur le message (`metadata.priority`) | Absent (ni envoi ni affichage des étiquettes reçues) | M | ★★ |
| Demande d'accusé de lecture | `C/post_view/acknowledgements`, `post_priority.button.acknowledge` | Bouton « Accuser réception », liste de qui a accusé (`POST /users/{u}/posts/{p}/ack`) | Absent | M | ★★ |
| Notifications persistantes | `C/persist_notification_confirm_modal.tsx`, `persist_notification.*` | Messages urgents relancés toutes les N minutes jusqu'à accusé | Absent (dépend du précédent) | S | ★ |
| Citer / répondre en citant | `post_info.quote`, `C/quote_selection_button` | « Citer » dans le menu ; bouton flottant pour citer une sélection | Absent (seul le format « citation » de l'éditeur existe) | S | ★★ |
| Marquer un fil comme non lu | `threading.threadMenu.markUnread` | Depuis la liste des fils | Absent (seul le message est marquable) | S | ★ |
| ~~Qui a réagi~~ | ~~`C/post_view/reaction`, `reaction.reacted`~~ | ~~Infobulle « X, Y et 3 autres ont réagi avec :+1: »~~ | ~~Absent~~ | ~~S~~ | ~~★★~~ |
| ~~Réactions rapides récentes~~ | ~~`C/post_view/post_recent_reactions`, `user.settings.display.oneClickReactionsOnPostsTitle`~~ | ~~Les 3 derniers emojis utilisés au survol~~ | ~~Partiel : 3 emojis figés (`QUICK_REACTIONS`)~~ | ~~S~~ | ~~★~~ |
| Déplacer un fil | `C/move_thread_modal`, `post_info.move_thread` | Déplace un fil vers un autre canal (droits requis) | Absent | M | ★ |
| ~~Réessayer un envoi échoué~~ | ~~`C/post_view/failed_post_options`~~ | ~~Message en échec affiché avec « Réessayer / Annuler »~~ | ~~Partiel : erreur générique (à vérifier : brouillon conservé ?)~~ | ~~M~~ | ~~★★~~ |

## 2. Rédaction et envoi

| Fonction | Où | Comportement officiel | Chez nous | Effort | Valeur |
|---|---|---|---|---|---|
| ~~Coller une image du presse-papiers~~ | ~~`file_upload.pasted`~~ | ~~Ctrl+V d'une capture : fichier « Image Pasted at … » joint~~ | ~~Absent (noté dans FONCTIONNALITES)~~ | ~~S~~ | ~~★★★~~ |
| ~~↑ pour modifier son dernier message~~ | ~~`shortcuts.msgs.edit`~~ | ~~Dans une saisie vide~~ | ~~Absent~~ | ~~S~~ | ~~★★★~~ |
| ~~Maj+↑ répondre au dernier message~~ | ~~`shortcuts.msgs.reply`~~ | ~~Ouvre le fil du dernier message~~ | ~~Absent~~ | ~~S~~ | ~~★~~ |
| ~~Envoyer avec Ctrl+Entrée~~ | ~~`user.settings.advance.sendTitle`, préférence `advanced_settings.send_on_ctrl_enter`~~ | ~~Entrée = nouvelle ligne ; Ctrl+Entrée envoie (tous messages ou seulement blocs de code)~~ | ~~Absent (préférence serveur ignorée)~~ | ~~S~~ | ~~★★~~ |
| Messages programmés (« Envoyer plus tard ») | `C/advanced_text_editor/send_button`, `C/drafts/scheduled_post_list`, `C/edit_scheduled_post`, `scheduled_post.*` | Menu du bouton d'envoi : demain 9 h, lundi, date libre ; onglet « Programmés » (modifier, replanifier, envoyer maintenant) ; indicateur dans le canal (`/posts/schedule`, serveur ≥ 10.3) | Absent | M | ★★ |
| Vue « Brouillons » | `C/drafts`, `drafts.*` | Liste de tous les brouillons (canaux et fils) : envoyer, modifier, supprimer | Partiel : brouillons par canal/fil + crayon, pas de vue d'ensemble | M | ★★ |
| Synchronisation des brouillons | `user.settings.advance.syncDrafts.*` (`/drafts`) | Brouillons stockés sur le serveur, retrouvés sur tous les appareils | Absent (brouillons locaux) | M | ★★ |
| Limite de longueur | `create_post.error_message` | Erreur « Character count: n/limit » avant envoi (`MaxPostSize`) | Absent | S | ★ |
| ~~Confirmation @channel / @all~~ | ~~`C/notify_confirm_modal.tsx`~~ | ~~« Envoyer une notification à N personnes dans M fuseaux ? » au-delà d'un seuil~~ | ~~Absent~~ | ~~S~~ | ~~★★~~ |
| ~~Avertissements en message direct~~ | ~~`C/advanced_text_editor/do_not_disturb_warning.tsx`, `out_of_office_warning.tsx`, `remote_user_hour.tsx`~~ | ~~Bandeau : la personne est en « Ne pas déranger », absente (répondeur), ou il est tard chez elle~~ | ~~Absent (l'heure locale existe dans la carte de profil)~~ | ~~S~~ | ~~★★~~ |
| ~~Mentions hors canal~~ | ~~`C/post_view/post_add_channel_member`, `post_body.check_for_out_of_channel_mentions.*`~~ | ~~Message éphémère « X n'a pas été notifié… l'ajouter au canal ? »~~ | ~~Absent (dépend du serveur : vérifier l'affichage du message éphémère)~~ | ~~S~~ | ~~★★~~ |
| Autocomplétion des groupes et mentions spéciales | `C/suggestion`, `C/advanced_text_editor/use_groups.tsx` | `@groupe`, `@here`, `@channel`, `@all` proposés | Partiel : personnes seulement | S | ★ |
| Progression des envois de fichiers | `C/file_upload`, `create_post.fileProcessing` | Barre de progression par fichier, annulable ; limite de nombre et de taille annoncée | Partiel : libellé « envoi en cours », pas de pourcentage ni d'annulation | S | ★ |
| Ctrl+U joindre un fichier | `shortcuts.files.upload` | Ouvre le sélecteur | Absent | S | ★ |
| Aide Markdown | `advanced_text_editor.help_link`, `C/help` | Lien vers l'aide de mise en forme | Absent | S | ★ |
| Éditeur riche (bêta), IA « Réécrire » | `C/advanced_text_editor/wysiwyg_editor`, `texteditor.rewrite.*` | WYSIWYG ; réécriture par l'agent IA (plugin Agents) | Absent — non prioritaire | L | ★ |

## 3. Fils, panneau droit

| Fonction | Où | Comportement officiel | Chez nous | Effort | Valeur |
|---|---|---|---|---|---|
| ~~Vue « Fils » globale~~ | ~~`C/threading/global_threads`, `threading.filters.unreads`, `threading.threadList.markRead`~~ | ~~Tous les fils suivis de l'équipe, onglet « Non lus », « Tout marquer comme lu », entrée en haut de la barre latérale avec pastille~~ | ~~Partiel : fils suivis du canal affiché seulement~~ | ~~M~~ | ~~★★★~~ |
| Fil dans une fenêtre séparée | `C/thread_popout`, `C/rhs_popout` | Détacher un fil ou le panneau droit | Absent | M | ★ |
| Agrandir le panneau droit | `shortcuts.nav.expand_sidebar` (Ctrl+Maj+.) | Panneau droit en grand | Absent | S | ★ |

## 4. Canaux, messages directs, équipes

| Fonction | Où | Comportement officiel | Chez nous | Effort | Valeur |
|---|---|---|---|---|---|
| ~~Parcourir les canaux~~ | ~~`C/browse_channels`, `more_channels.*`~~ | ~~Liste des canaux publics, archivés, privés « découvrables » ; filtres, masquer ceux rejoints, « Rejoindre », demande d'adhésion~~ | ~~Absent (seule la création existe)~~ | ~~M~~ | ~~★★★~~ |
| ~~Nouveau message direct / de groupe~~ | ~~`C/more_direct_channels`, `shortcuts.nav.direct_messages_menu` (Ctrl+Maj+K)~~ | ~~« + » de la catégorie Messages directs : choisir 1 à 7 personnes~~ | ~~Partiel : message direct depuis la carte de profil, pas de groupe~~ | ~~M~~ | ~~★★★~~ |
| Fermer une conversation directe | `sidebar_left.sidebar_channel_menu.leaveConversation` | Retire le MP de la barre latérale | Absent | S | ★★ |
| ~~Préférences de notification du canal~~ | ~~`C/channel_notifications_modal`, `channel_notifications.*`~~ | ~~Bureau/mobile : tout, mentions, rien ; réponses de fils ; suivre tous les fils du canal ; ignorer @channel ; son ; réinitialiser (`PUT /channels/{c}/members/{u}/notify_props`)~~ | ~~Partiel : sourdine seulement (les réglages existants sont lus)~~ | ~~M~~ | ~~★★★~~ |
| ~~Paramètres du canal~~ | ~~`C/channel_settings_modal`, `channel_settings.tab.*`~~ | ~~Renommer, changer l'URL, convertir en privé, archiver / désarchiver~~ | ~~Partiel : description et en-tête modifiables~~ | ~~M~~ | ~~★★~~ |
| ~~Canaux archivés~~ | ~~`C/center_message_lock`, `C/unarchive_channel_modal`~~ | ~~Lecture seule avec bandeau, recherche dans les archivés~~ | ~~Absent (non géré)~~ | ~~S~~ | ~~★~~ |
| ~~Gestion des membres~~ | ~~`C/channel_members_rhs`, `channel_members_rhs.action_bar.manage_button`~~ | ~~« Gérer » : retirer, nommer administrateur~~ | ~~Partiel : liste et ajout seulement~~ | ~~S~~ | ~~★★~~ |
| Menu de canal complet | `sidebar_left.sidebar_channel_menu.*` | « Déplacer vers… » (catégorie existante ou nouvelle), « Marquer comme non lu » | Partiel : marquer lu, favori, sourdine, lien, quitter ; déplacement par glisser-déposer seulement | S | ★★ |
| ~~Gestion des catégories~~ | ~~`sidebar_left.sidebar_category_menu.*`~~ | ~~Renommer, supprimer une catégorie~~ | ~~Absent (créer, trier, sourdine, marquer lu existent)~~ | ~~S~~ | ~~★★~~ |
| Filtre « Non lus seulement » | `sidebar_left.channel_filter.*`, Ctrl+Maj+U | Bascule non lus / tous dans la barre latérale | Partiel : section « Non lus » si la préférence est active | S | ★ |
| Convertir un MP de groupe en canal | `C/convert_gm_to_channel_modal` | Transforme une conversation de groupe en canal privé | Absent | M | ★ |
| Signets de canal | `C/channel_bookmarks`, `channel_bookmarks.*` | Barre de liens/fichiers sous l'en-tête (ajout, ordre, emoji) | Absent (pas même l'affichage) | M | ★★ |
| Bannière de canal | `C/channel_banner` | Texte coloré en haut du canal (Entreprise) | Absent | S | ★ |
| ~~Liens internes dans l'application~~ | ~~`C/permalink_view`, `C/inline_entity_link`~~ | ~~Un lien `…/équipe/pl/id` ou `…/channels/nom` s'ouvre dans le client (canal, fil, message en surbrillance)~~ | ~~Absent : `open_link` envoie tout au navigateur~~ | ~~S~~ | ~~★★★~~ |
| Équipes | `navbar_dropdown.join`, `navbar_dropdown.leave`, `C/team_settings_modal` | Rejoindre une autre équipe, quitter, paramètres de l'équipe | Partiel : description seulement | M | ★ |

## 5. Statut et notifications

| Fonction | Où | Comportement officiel | Chez nous | Effort | Valeur |
|---|---|---|---|---|---|
| ~~Statut personnalisé~~ | ~~`C/custom_status`, `custom_status.*`, `C/post_view/post_header_custom_status`~~ | ~~Emoji + texte, suggestions (« En réunion »…), récents, expiration (30 min → date libre) ; affiché à côté du nom dans les messages, la carte de profil, les membres (`PUT /users/me/status/custom`)~~ | ~~Absent (ni réglage ni affichage)~~ | ~~M~~ | ~~★★★~~ |
| « Ne pas déranger » avec durée | `userAccountMenu.dndSubMenu.*`, `C/dnd_custom_time_picker_modal` | 30 min, 1 h, 2 h, demain, personnalisé (`dnd_end_time`) | Partiel : statut sans échéance | S | ★★ |
| Paramètres de notification | `C/user_settings/notifications`, `user.settings.notifications.*` | Bureau (tout/mentions/rien, fils suivis), sons, e-mail, mots-clés qui notifient / surlignent, réponses, suivi auto sur @channel | Absent (préférences lues mais non modifiables) | M | ★★ |
| Réponse automatique (absence) | `user.settings.notifications.autoResponder*` | Message envoyé automatiquement en MP | Absent | S | ★ |
| Sons de notification | `user.settings.notifications.desktopNotificationSound.*` | Choix du son (Bing, Crackle…) | Absent (son Windows par défaut) | S | ★ |
| Mots-clés surlignés | `user.settings.notifications.keywordsWithHighlight.*` | Mots surlignés sans notifier (Professional) | Absent | S | ★ |

## 6. Paramètres utilisateur (Ctrl+Maj+A)

Aucun écran de paramètres dans matterfew : les préférences serveur sont lues
(thème, catégories, non lus, nombre de MP) mais rien n'est modifiable.

| Réglage | Où (`user.settings.*`) | Chez nous | Effort | Valeur |
|---|---|---|---|---|
| Fenêtre « Paramètres » (onglets Notifications, Affichage, Barre latérale, Avancé) | `C/user_settings` | Absent | L (ensemble) | ★★ |
| Affichage du nom (pseudo, nom complet, identifiant) | `display.teammateNameDisplay*` | Absent : règle fixe (pseudo sinon nom complet) | S | ★★ |
| Horloge 12 h / 24 h, fuseau | `display.clockDisplay`, `display.timezone` | Absent (format système) | S | ★ |
| Fils réduits (CRT) on/off | `display.collapsedReplyThreads*` | Partiel : la préférence est respectée, pas modifiable | S | ★ |
| Aperçus de liens on/off, images dépliées par défaut | `display.linkPreviewDisplay`, `display.collapseDisplay` | Absent (toujours affichés) | S | ★ |
| Affichage compact, coloration des noms | `display.messageDisplay*`, `display.colorize` | Absent | M | ★ |
| Messages d'arrivée/départ | `advance.joinLeaveTitle` | À vérifier (préférence non lue) | S | ★ |
| Position à l'ouverture d'un canal non lu | `advance.unreadScrollPosition*` | Absent (toujours au séparateur) | S | ★ |
| Langue de l'interface | `languages.*` | Partiel : langue du système (fr, en, pt-BR) | S | ★ |
| Choix du thème | `display.theme.*` | Partiel : thème lu, pas de sélecteur | M | ★ |
| Profil : nom, pseudo, poste, photo | `general.*` | Absent | M | ★ |
| Sécurité : mot de passe, MFA, sessions, jetons | `security.*`, `mfa.*` | Absent | M | ★ |

## 7. Rendu des messages et fichiers

| Fonction | Où | Comportement officiel | Chez nous | Effort | Valeur |
|---|---|---|---|---|---|
| Aperçu PDF | `C/pdf_preview.tsx`, `C/file_preview_modal` | PDF lu dans la visionneuse | Absent (ouvrir/télécharger) | M | ★★ |
| Aperçu de code / texte | `C/code_preview.tsx` | Fichiers source affichés avec coloration | Absent | S | ★ |
| Lecteur vidéo / audio | `C/audio_video_preview`, `C/youtube_video` | Lecture intégrée ; vidéos YouTube intégrées | Absent (vidéo listée dans les médias, ouverte à l'extérieur) | L | ★ |
| Visionneuse : fichier suivant, lien public | `file_preview_modal_main_nav.*`, `file_preview_modal_main_actions.*` | Flèches « n sur N », « Copier le lien public » | Partiel : une image à la fois | S | ★★ |
| Masquer un aperçu | `link_preview.remove_link_preview`, `link_preview.hide_image_preview` | Retirer l'aperçu de lien de son message, replier une image | Absent | S | ★ |
| LaTeX | `C/latex_block`, `C/latex_inline` | ```` ```latex ```` et `$…$` (si activé par l'admin) | Absent | M | ★ |
| Coloration syntaxique des blocs de code | `C/code_block` | Langages courants, bouton copier | À vérifier (dépend du `TextView` de GPUI Kit) | S | ★★ |
| Listes de tâches | Markdown | Cases `[ ]` / `[x]` | Probablement présent (`ENABLE_TASKLISTS` dans `markdown.rs`) — à vérifier à l'écran | — | — |
| Messages système regroupés | `C/post_view/combined_user_activity_post` | « A, B et 3 autres ont rejoint le canal » | À vérifier | S | ★ |
| Listes déroulantes et dialogues interactifs | `C/interactive_dialog`, `C/apps_form`, `C/post_view/message_attachments` | Menus `select` dans les cartes, formulaires ouverts par une commande ou un bouton (`open_dialog`) | Partiel : boutons seulement ; ni `select` ni dialogues | M | ★★ |
| Émoticônes en emojis | `display.renderEmoticonsAsEmojiTitle` | `:D` → 😄 | À vérifier | S | ★ |

## 8. Emojis, GIF

| Fonction | Où | Comportement officiel | Chez nous | Effort | Valeur |
|---|---|---|---|---|---|
| ~~Emojis récents~~ | ~~`emoji_picker.recent` (préférence `recent_emojis`)~~ | ~~Catégorie « Récemment utilisés » en tête, partagée entre appareils~~ | ~~Absent~~ | ~~S~~ | ~~★★~~ |
| Couleur de peau | `emoji_picker.skin_tone` | Sélecteur mémorisé | Absent (noms reconnus seulement) | S | ★ |
| GIF | `C/gif_picker` | Recherche Giphy/Tenor via le serveur (si activé) | Absent | M | ★ |
| Réagir au dernier message | `shortcuts.msgs.comp.last_reaction` (Ctrl+Maj+\\) | Ouvre le sélecteur pour le dernier message | Absent | S | ★ |
| Ajouter un emoji personnalisé | `C/emoji`, `add_emoji.*` | Téléverser une image | Absent | M | ★ |

## 9. Recherche

| Fonction | Où | Comportement officiel | Chez nous | Effort | Valeur |
|---|---|---|---|---|---|
| Recherche de fichiers | `search_bar.files_tab`, `C/file_search_results`, `search_bar.file_types` | Onglet « Fichiers » + filtre par type (documents, images, code…) | Partiel : fichiers du canal ; pas dans la recherche globale | S | ★★ |
| Sélecteur de date | `C/date_picker` | Calendrier pour `on:` / `before:` / `after:` | Partiel : suggestions toutes prêtes, dates tapées | S | ★ |
| Filtres des mentions récentes | `search_results.mention_filters.*` | Équipe, canal, date, type de mention | Absent | S | ★ |
| Filtre de type dans les fichiers du canal | `channel_info_rhs.menu.files.filter.*` | Documents, tableurs, code… | Partiel : onglets Médias / Fichiers / Liens | S | ★ |

## 10. Raccourcis clavier

Chez nous : Ctrl+K, Ctrl+F, Échap, mise en forme (Ctrl+B/I/Maj+X/Alt+K/Alt+C),
↑ ↓ Entrée Tab dans l'autocomplétion. Liste officielle : `shortcuts.*`, `C/keyboard_shortcuts`.

| Raccourci | Effet | Effort | Valeur |
|---|---|---|---|
| ~~Alt+↑ / Alt+↓~~ | ~~Canal précédent / suivant~~ | ~~S~~ | ~~★★~~ |
| ~~Alt+Maj+↑ / ↓~~ | ~~Canal non lu précédent / suivant~~ | ~~S~~ | ~~★★★~~ |
| ~~Alt+← / Alt+→~~ | ~~Historique de navigation~~ | ~~S~~ | ~~★★~~ |
| ~~Ctrl+Maj+M~~ | ~~Mentions récentes~~ | ~~S~~ | ~~★~~ |
| ~~Ctrl+Maj+L~~ | ~~Focus dans la zone de saisie~~ | ~~S~~ | ~~★~~ |
| Ctrl+. / Ctrl+Alt+I | Ouvrir/fermer le panneau droit / infos du canal | S | ★ |
| ~~Ctrl+Alt+↑ / ↓, Ctrl+Alt+1…9~~ | ~~Équipe précédente / suivante / n~~ | ~~S~~ | ~~★~~ |
| ~~Maj+Échap~~ | ~~Tout marquer comme lu (avec confirmation `C/mark_all_as_read_modal`)~~ | ~~S~~ | ~~★★~~ |
| Échap (dans un canal) | Marquer le canal comme lu | S | ★ |
| Ctrl+↑ / ↓ | Rappeler un message envoyé précédemment | S | ★ |
| ~~Ctrl+/~~ | ~~Fenêtre d'aide des raccourcis~~ | ~~S~~ | ~~★★~~ |
| Ctrl+ + / – | Zoom de l'interface | M | ★★ |

## 11. Profil et personnes

| Fonction | Où | Comportement officiel | Chez nous | Effort | Valeur |
|---|---|---|---|---|---|
| Actions de la carte de profil | `C/profile_popover`, `user_profile.*` | « Ajouter à un canal », appel, statut personnalisé, « Modifier le profil » pour soi, dernière connexion | Partiel : « Envoyer un message » seulement | S | ★ |
| « Dernière connexion » | `channel_header.lastOnline` | Dans l'en-tête d'un MP | Absent | S | ★ |
| Groupes d'utilisateurs | `C/user_groups_modal`, `C/user_group_popover` | Voir les groupes, leurs membres (`@groupe`) | Absent | M | ★ |

## 12. Robustesse, accessibilité, langue

| Fonction | Où | Comportement officiel | Chez nous | Effort | Valeur |
|---|---|---|---|---|---|
| ~~Bandeau hors ligne~~ | ~~`announcement_bar.warn.no_internet_connection`~~ | ~~Bandeau visible en haut de l'écran~~ | ~~Partiel : « Reconnexion… » à la place du statut dans la barre latérale~~ | ~~S~~ | ~~★★~~ |
| ~~Toasts du fil de messages~~ | ~~`C/toast_wrapper`, `postlist.toast.*`~~ | ~~« N nouveaux messages », « Aller aux non lus », « Vous consultez l'historique »~~ | ~~Partiel : « Revenir au présent »~~ | ~~S~~ | ~~★~~ |
| Accessibilité | `accessibility.sections.*`, `C/a11y` (régions, F6) | Lecteur d'écran, navigation par régions | Absent / limité par GPUI | L | ★★ |
| Traductions | `webapp/channels/src/i18n/*.json` | ≈ 20 langues | Partiel : fr, en, pt-BR | M | ★ |

## 13. À mentionner seulement (plugins, éditions payantes, récent)

Appels (plugin Calls, `C/call_options_menu`, `shortcuts.calls.*`) ; Playbooks ;
Boards (« board » partout dans `en.json` : les canaux deviennent des tableaux) ;
IA : résumés « Recaps » (`C/recaps`), réécriture, plugin Agents ; « Burn on read »
(messages éphémères, `C/burn_on_read`, serveur 11) ; traduction automatique des
canaux (`channel_header.autotranslation.*`, `C/show_translation_modal`) ; canaux
partagés (`C/shared_channel_indicator.tsx`) ; attributs de canal et de message
(`C/channel_attributes`, `C/post_attributes`) ; barre d'applications des plugins
(`C/app_bar`), boutons d'en-tête et entrées de menu ajoutés par les plugins ;
Marketplace ; Insights (retiré).

---

## Top 15 à implémenter ensuite

Classement : valeur pour un utilisateur venant du client officiel, puis effort.
✅ : fait (toutes les priorités ★★★ le sont).

| # | Fonction | Valeur | Effort | Remarque |
|---|---|---|---|---|
| ✅ 1 | ~~Liens internes ouverts dans l'application (permaliens, `~canal`)~~ | ~~★★★~~ | ~~S~~ | ~~Aujourd'hui tout part dans le navigateur ; réutilise « ouvrir le canal et le fil » de la recherche~~ |
| ✅ 2 | ~~Coller une image (Ctrl+V)~~ | ~~★★★~~ | ~~S~~ | ~~Upload déjà en place~~ |
| ✅ 3 | ~~↑ pour modifier son dernier message~~ | ~~★★★~~ | ~~S~~ | ~~Édition dans la zone de saisie déjà en place~~ |
| ✅ 4 | ~~Alt+↑/↓ et Alt+Maj+↑/↓ (canaux, canaux non lus)~~ | ~~★★★~~ | ~~S~~ | ~~Ordre déjà calculé par `sidebar.rs`~~ |
| ✅ 5 | ~~Suivre / ne plus suivre un fil~~ | ~~★★★~~ | ~~S~~ | ~~Deux appels REST~~ |
| ✅ 6 | ~~Rappel « Me le rappeler »~~ | ~~★★★~~ | ~~S~~ | ~~Un appel REST + sous-menu~~ |
| ✅ 7 | ~~Préférences de notification par canal~~ | ~~★★★~~ | ~~M~~ | ~~`notify_rules.rs` lit déjà ces propriétés~~ |
| ✅ 8 | ~~Parcourir et rejoindre les canaux (publics, archivés)~~ | ~~★★★~~ | ~~M~~ | |
| ✅ 9 | ~~Nouveau message direct / de groupe (Ctrl+Maj+K)~~ | ~~★★★~~ | ~~M~~ | |
| ✅ 10 | ~~Statut personnalisé (affichage puis réglage) + « Ne pas déranger » avec durée~~ | ~~★★★~~ | ~~M~~ | ~~Statut fait ; « Ne pas déranger » avec durée reste à faire (★★)~~ |
| ✅ 11 | ~~Vue « Fils » globale (équipe, non lus, tout marquer lu)~~ | ~~★★★~~ | ~~M~~ | ~~Panneau par canal existant à généraliser~~ |
| ~~12~~ | ~~Envoi avec Ctrl+Entrée (préférence serveur)~~ | ~~★★~~ | ~~S~~ | |
| ~~13~~ | ~~Qui a réagi + emojis récents (réactions rapides et sélecteur)~~ | ~~★★~~ | ~~S~~ | |
| 14 | Priorité, étiquettes Important/Urgent et accusés de lecture | ★★ | M | Afficher les étiquettes reçues d'abord (S) |
| 15 | Messages programmés + vue Brouillons | ★★ | M | Brouillons locaux déjà là ; serveur ≥ 10.3 pour la programmation |

Juste après : transfert de message, historique des modifications, aperçu PDF,
recherche de fichiers globale, avertissements en MP (Ne pas déranger, heure
tardive), fenêtre « Paramètres », raccourcis restants et aide Ctrl+/.

## Points non vérifiés

- Coloration syntaxique des blocs de code et rendu des listes de tâches : dépendent du
  `TextView` de GPUI Kit, non testé (l'application n'a pas été lancée).
- Messages d'arrivée/départ regroupés, émoticônes `:D`, affichage des messages
  éphémères « mention hors canal » : non confirmés dans le code.
- Comportement après un envoi en échec (brouillon conservé ?).
- Disponibilité côté serveur : messages programmés (≥ 10.3), signets, priorité/accusés
  (option admin `PostPriority`), « Burn on read » et traduction automatique (≥ 11,
  éditions payantes) — à confronter à la version du serveur cible.
