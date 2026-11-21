# Design system Mattermost (Compass) : ce qu'on peut reprendre

Relevé du 21/11/2026, à partir du code source (valeurs CSS exactes), pas de captures.
Les mentions **(déduit)** signalent une interprétation ou une recommandation, pas une valeur lue.

## 0. Les sources et leur fiabilité

Mattermost a aujourd'hui **trois couches** de design, d'âges différents :

| Couche | Statut | Usage ici |
|---|---|---|
| **SCSS historique du webapp** (`webapp/channels/src/sass/**`, branche `master`) | Ce que les utilisateurs voient aujourd'hui | Valeurs « en prod » |
| **`@mattermost/compass-ui`** (paquet npm, version `0.1.0-alpha.28` épinglée par le webapp) | Nouvelle bibliothèque Compass, en cours d'intégration. La barre latérale du webapp ne l'utilise pas encore (elle importe encore ses composants maison) | Spécification « cible », la plus précise |
| **`mattermost/compass-components`** (GitHub, **archivé en 2022**) | Ancienne lib React. Échelles d'origine (typo, espacements, élévation) | Pour l'historique et les échelles |

Abréviations utilisées dans tout le document (liens complets) :

- **[WV]** variables CSS : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/base/_css_variables.scss
- **[WMX]** mixins (boutons, focus) : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/utils/_mixins.scss
- **[WSB]** barre latérale : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/layout/_sidebar-left.scss
- **[WBG]** badges : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/components/_badge.scss
- **[WP]** messages : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/components/_post.scss
- **[WPM]** menu de survol d'un message : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/components/_post-menu.scss
- **[WR]** réactions : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/components/post_view/reaction/reaction.scss
- **[WT]** infobulles : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/components/_tooltip.scss
- **[WH]** en-tête de canal : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/layout/_headers.scss
- **[WS]** structure de la page : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/base/_structure.scss
- **[WTY]** typographie : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/base/_typography.scss
- **[WMD]** titres markdown : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/layout/_markdown.scss
- **[WBT]** boutons `.btn` : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/components/_buttons.scss
- **[WTS]** barre d'équipes : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/sass/layout/_team-sidebar.scss
- **[WU]** `applyTheme()` (couleurs dérivées) : https://github.com/mattermost/mattermost/blob/master/webapp/channels/src/utils/utils.tsx
- **[CU]** jetons compass-ui : https://unpkg.com/@mattermost/compass-ui@0.1.0-alpha.28/dist/compass-ui.css
- **[CU:X]** composant compass-ui `X` : `https://unpkg.com/@mattermost/compass-ui@0.1.0-alpha.28/dist/components/<x>/<X>.css` (par ex. [CU:ChannelSidebarItem](https://unpkg.com/@mattermost/compass-ui@0.1.0-alpha.28/dist/components/channel-sidebar-item/ChannelSidebarItem.css))
- **[CC]** compass-components (archivé) : https://github.com/mattermost/compass-components/tree/master/src
- **[DOC-KB]** https://docs.mattermost.com/end-user-guide/collaborate/keyboard-accessibility.html et https://docs.mattermost.com/end-user-guide/collaborate/keyboard-shortcuts.html

Le Figma « Compass Design System » n'est pas consultable sans compte. Les pages design de developers.mattermost.com n'ont pas été trouvées (404).

---

## 1. Couleurs

### 1.1 Principe : tout part des couleurs du thème, déclinées en opacité

Le webapp expose chaque couleur du thème en hexadécimal **et** en triplet RGB (`--center-channel-color-rgb: 61, 60, 64`), puis compose toutes les nuances avec `rgba(var(--x-rgb), α)` [WV], [WU]. Il n'y a quasiment pas de couleurs en dur : survol, bordures, textes secondaires et désactivés sont tous **texte du thème + opacité**.

Échelle d'opacités officielle de compass-ui [CU] :
`0.04, 0.08, 0.12, 0.16, 0.20, 0.24, 0.32, 0.40, 0.48, 0.56, 0.64, 0.72, 0.80, 0.88, 0.96, 1`.

Rôles observés (zone centrale, base `center_channel_color`) :

| α | Rôle | Source |
|---|---|---|
| 0.04 | survol d'un message | [WP] `.post:hover` |
| 0.08 | survol d'un bouton-icône ou d'un élément de menu, fond de puce de réaction, fond de bouton désactivé, bordure légère (`--border-light`) | [WMX], [CU:MenuItem], [WR], [WV] |
| 0.12 | bordure par défaut (`--border-default`), séparateurs, survol d'une réaction | [WV], [CU:MessageSeparator], [CU:ReactionButton] |
| 0.16 | bordure marquée (`--border-dark`), bordure de menu ou de modale, champ de recherche au repos | [WV], [CU:PopoverMenu], [CU:SearchInput] |
| 0.24 | champ de recherche survolé, bordure de la zone de saisie | [CU:SearchInput], [WP] |
| 0.32 | texte ou icône désactivé, statut « hors ligne » | [WMX], [CU:StatusBadge] |
| 0.40 | contenu désactivé dans un menu, canal en sourdine | [CU:MenuItem], [CU:ChannelSidebarItem] |
| 0.56 | horodatage, compteur de réaction | [CU:MessageHeader], [CU:ReactionButton] |
| 0.64 | icônes (`--icon-opacity`), libellé d'un champ, texte secondaire | [WV], [CU:SearchInput] |
| 0.72 / 0.75 | sous-titres, raccourcis, titres de groupe de menu | [CU:ModalHeader], [CU:MenuGroupHeading] |
| 0.80 | icône survolée (`--icon-opacity-hover`) | [WV] |

Barre latérale (base `sidebar_text`) : 0.08 survol (compass-ui), 0.12 canal actif (compass-ui) ou 0.08 (webapp), 0.16 fond d'icône de groupe, 0.64 texte au repos (webapp) ou 0.72 (compass-ui) [WSB], [CU:ChannelSidebarItem].

États d'action (bouton secondaire ou tertiaire, élément actif) : base `button_bg` (webapp) ou `link_color` (compass-ui) à **0.08 au repos, 0.12 au survol, 0.16 à l'appui** [WMX], [CU:Button]. Bouton primaire : **mélange de 8 % de noir au survol et 16 % à l'appui** (`color-mix(in srgb, black 8%, var(--button-bg))`) [CU:Button]. Le webapp obtient la même chose par superposition d'un dégradé `rgba(0,0,0,.08)` [WMX].

### 1.2 Couleurs dérivées opaques (applyTheme) [WU]

- `mention-highlight-bg-mixed` = mélange `center_bg` / `mention_highlight_bg` à 50 %.
- `pinned-highlight-bg-mixed` = même mélange à **24 %**, en fond opaque des messages épinglés ou enregistrés ([WP] `.post--pinned-or-flagged`).
- `own-highlight-bg` = `mention_highlight_bg` mélangé à 5 % de `center_color`.
- Focus clavier (`.a11y--focused`) : `box-shadow: inset 0 0 1px 3px link@0.5, inset 0 0 0 1px link`.
- Couleurs sémantiques fixes, indépendantes du thème : info `93,137,234`, succès `61,184,135`, avertissement `204,143,0`, danger `210,75,78` [WV].
- Hors ligne, identique dans tous les thèmes : `rgba(175,179,192,0.75)` [WV].

### 1.3 Élévation (ombres) [WV], [CU]

| Niveau | Ombre | Usage observé |
|---|---|---|
| 1 | `0 2px 3px rgba(0,0,0,.08)` | menu de survol d'un message, carte centrale |
| 2 | `0 4px 6px rgba(0,0,0,.12)` | élément glissé (canal en cours de déplacement) |
| 3 | `0 6px 14px rgba(0,0,0,.12)` | infobulle |
| 4 | `0 8px 24px rgba(0,0,0,.12)` | menu |
| 5 | `0 12px 32px rgba(0,0,0,.12)` | sous-menu |
| 6 | `0 20px 32px rgba(0,0,0,.12)` | modale |

Dans compass-components, les thèmes sombres portent l'opacité des ombres à **0.32** (0.08 en clair) [CC] `utilities/elevation/Elevation.mixins.ts`. Fond derrière une modale : `rgba(neutral-1200, 0.64)`, où neutral-1200 = `#090a0b` [CU].

### 1.4 Palette Compass (pour les couleurs hors thème)

Neutres `#ffffff` → `#090a0b` en 25 pas. Bleu 500 `#1c58d9` (= bouton Denim), indigo 500 `#1e325c` (= barre latérale Denim), vert 500 `#3db887`, rouge 400 `#d24b4e`, jaune 400 `#ffbc1f` [CU]. Utile pour les **avatars sans photo**, dont les fonds possibles sont red-500, purple-500, neutral-500, neutral-900, indigo-500, teal-700, blue-500, cyan-500, yellow-500, green-500 et orange-500, avec un texte blanc à 92 % en Metropolis semi-gras [CU:UserAvatar].

---

## 2. Typographie

- Corps de texte : **Open Sans**. Titres (h1–h3, titre du canal, titre de modale) : **Metropolis 600** [WTY], [CU] (`--font-family-heading`, `--font-family-body`). Police à chasse fixe : Menlo [CU].
- Graisses : 300, 400, **600 (semi-gras, l'emphase standard, y compris `<b>`)** et 700 [CU], [WTY].

Échelle (taille / interligne) [CU], identique à compass-components [CC] `components/text/Text.constants.ts` :

| Jeton | 25 | 50 | 75 | **100** | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900 | 1000 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| px | 10 | 11 | 12 | **14** | 16 | 18 | 20 | 22 | 25 | 28 | 32 | 36 | 40 |
| interligne | 16 | 16 | 16 | **20** | 24 | 24 | 28 | 28 | 30 | 36 | 40 | 44 | 48 |

Affectations relevées :

| Élément | Valeur | Source |
|---|---|---|
| Texte de message | 14 / 20 (jeton 100). Le webapp historique écrit `line-height: 1.5` sur `.post__body` | [CU:MessageHeader], [WP] |
| Nom d'auteur | 14 / 20, 600 | [CU:MessageHeader] |
| Horodatage | **12 / 16, 400, center_color à 0.56** | [CU:MessageHeader] |
| Canal dans la barre latérale | 14 / 20, 400, **letter-spacing 0.02em**, opacité 0.72. Non lu : 600, opacité 1 | [CU:ChannelSidebarItem] |
| En-tête de catégorie | **12 px, 600, MAJUSCULES, letter-spacing 0.05em**, sidebar_text à 0.64 | [WSB] |
| Titre de canal (en-tête) | Metropolis 16 px, 600, interligne 16 | [WH] |
| Badge de mention | 11 px, 600 (compass-ui) ou 700 (webapp), interligne 16 | [CU:MentionBadge], [WBG] |
| Infobulle | 12 / 16, 600, blanc | [CU:Tooltip] |
| Élément de menu | 14 / 20, 400. Libellé secondaire 12 / 16 à 75 % | [CU:MenuItem] |
| Titre de groupe de menu | 11 / 16, 600, majuscules, center_color à 0.64 | [CU:PopoverMenu] |
| Titre de modale | Metropolis 22 / 28, 600 | [CU:ModalHeader] |
| Séparateur (date, nouveaux messages) | 12 / 16, 600 | [CU:MessageSeparator] |
| Titres markdown h1 à h6 | 28 / 25 / 22 / 19 / 15 / 1em | [WMD] |

---

## 3. Espacements et dimensions

- **Grille de 4 px avec demi-pas** : jetons `2, 4, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48` [CU] (`--spacing-xxxxs` → `--spacing-xxxxxl`). compass-components : `0, 2, 4, 6, 8, 10, 12, 16, 20, 24, 28, 32, 40, 48, 56, 64, 72, 80` [CC] `utilities/spacing/Spacing.constants.ts`.
- **Rayons** : 2 (xs), **4 (s : boutons, puces, éléments de menu, champs)**, **8 (m : menus, popovers)**, **12 (l : modales, carte centrale)**, 16 (xl), pilule 9999 [CU], [WV].
- **Icônes** : 10, 12, **16** (barre latérale, menus), **20** (défaut), 24, 28, 32… [CU], [CC] `foundations/icon/Icon.constants.ts`.
- **Durées** : rapide 150 ms et modérée 300 ms. Courbes : `ease-in-out` (transition), `ease-out` (entrée), `ease-in` (sortie) [CU]. Webapp : `$transition-quick: 0.12s`, animation de la hauteur des canaux 0.18 s [WSB].

| Gabarit | Valeur | Source |
|---|---|---|
| Barre latérale | largeur min 240 px | [WSB] |
| Barre d'équipes | 65 px (305 − 240), icônes d'équipe 32 à 40 px, rayon 6 à 8 px | [WSB], [WTS], [CU:TeamAvatar] |
| Ligne de canal | **32 px** de haut | [WSB], [CU:ChannelSidebarItem] |
| Marge gauche de la ligne de canal | 18 px au repos, 16 px si active (car elle porte la barre d'activité de 2 px) | [CU:ChannelSidebarItem] |
| Marge droite de la ligne de canal | 16 px | [CU:ChannelSidebarItem] |
| En-tête de catégorie | 32 px de haut, padding gauche 16 px, 6 px sous chaque groupe | [WSB] |
| En-tête de canal | hauteur max 56 px, padding 12 px, rayon 12 12 0 0, bordure inférieure `--border-default` | [WH] |
| Carte centrale | `margin: 0 4px 4px`, rayon 12, `--border-light`, élévation 1 | [WS] |
| Message | padding `8px 0.5em 0 1.5em`. Colonne avatar de **53 px** (32 px + 10 px de padding droit) | [WP] |
| Contenu de message | largeur max 1000 px | [WP] |
| Avatars | 12 à 128 px. Usages : **20** (MP dans la barre latérale), 24, **32** (message), 40, 48, 72… | [CU:UserAvatar], [CC] `components/avatar/Avatar.constants.ts` |

---

## 4. Composants

### 4.1 Badge de mention (compteur)

- **compass-ui, taille « medium » utilisée par la barre latérale** [CU:MentionBadge] (la taille est lue dans `ChannelSidebarItem.js`) : 11 px, 600, interligne 16 → **16 px de haut**, letter-spacing 0.22 px, **rayon 8 px**, fond `mention_bg`, texte `mention_color`. Le padding horizontal dépend du nombre de chiffres : **6.5 px (1 chiffre), 5.5 px (2), 4.5 px (3, soit « 99+ »)**. Au-delà de 99, le badge affiche **« 99+ »**, avec `aria-label` « N mentions ».
- Taille « large » : 12 px, interligne 15, padding 1.5 px × 7.5 / 5.5 / 4 px, pilule. Taille « small » : 10 px, interligne 11, padding 0.5 × 4.5 px.
- **Webapp actuel** [WBG] : `min-width: 20px`, `padding: 0 6px`, `border-radius: 8px`, 11 px, **700**, interligne 16, `margin: 0 4px`.
- **Badge urgent** : fond `dnd_indicator`, texte blanc. Il **reste visible au survol**, alors que les autres sont masqués [WBG], [WSB].
- **Barre d'équipes** [WBG] : badge en `top: -3px; right: -3px`, **entouré d'un anneau `box-shadow: 0 0 0 3px <fond de la barre d'équipes>`** qui le détache de l'icône.
- Selon l'emplacement [CU:MentionBadge] : dans un menu, fond center_color à 0.48 ; dans un bouton-icône, fond `sidebar_text_active_border` ; dans le canal, fond `button_bg`.
- **Point non lu** [CU:UnreadBadge] : pastille de 6 ou 8 px, pilule. Fond `mention_bg` sur une icône d'équipe, `button_bg` sur un bouton-icône. Sur un bouton-icône : 6 px, `sidebar_text_active_border`, anneau de 2 px de la couleur du fond [CU:IconButton].

### 4.2 Ligne de canal (barre latérale)

| État | compass-ui (cible) [CU:ChannelSidebarItem] | Webapp actuel [WSB] |
|---|---|---|
| Repos | nom à 0.72, icône à 0.64, icône 16 px dans un padding de 4 px | texte à 0.64, icône 18 px avec 6 px d'écart |
| Survol | fond **sidebar_text à 0.08**. **Le badge disparaît (fondu de 150 ms) au profit du bouton « ⋯ »** | fond `sidebar_text_hover_bg`, badge masqué (sauf urgent), bouton de menu affiché |
| Actif | fond **sidebar_text à 0.12**, nom à opacité 1, **barre de 2 px** `sidebar_text_active_border` à gauche, rayon `0 2px 2px 0` | fond sidebar_text à 0.08, **barre de 4 px**, `left: -2px`, rayon 4 |
| Non lu / mention | nom en **600**, opacité 1, icône à 1 | `sidebar_unread_text`, 600 |
| Sourdine | **icône, nom et badge à 0.4** | icône, libellé, badge et statut à 0.4 |
| Brouillon | icône crayon à la place du badge, nom à 0.72 | — |
| Focus clavier | `inset 0 0 0 2px sidebar_text_active_border` | `.a11y--focused` |

Pour les messages directs, l'avatar fait 20 px et la pastille de statut 13 px, posée sur un fond `sidebar_bg` [WSB].

### 4.3 En-tête de catégorie [WSB]

32 px de haut. Texte en 12 px, 600, majuscules, letter-spacing 0.05em, sidebar_text à 0.64. Chevron de 16 px max, qui pivote de −90° en 0.15 s. Boutons de tri et « + » de 28 px, cachés (`opacity: 0`) et révélés au survol ou au focus. Catégorie en sourdine : texte à 0.4. Pastille « NEW » : 16 px de haut, 10 px, 600, majuscules, rayon 4.

### 4.4 Message, menu de survol, réactions

- **Survol d'un message** : fond center_color à **0.04**, transition de 200 ms [WP]. Message en cours d'édition : fond `button_bg` à 0.08 [WP]. Message ciblé par un permalien : fond `mention_highlight_bg` à 0.5, qui s'estompe après 5 s, sur 2 s [WP]. Message épinglé ou enregistré : fond opaque `pinned-highlight-bg-mixed` [WP], [WU]. **Le webapp ne colore pas les messages de l'utilisateur lui-même** : rien de tel dans [WP].
- **Réponse dans un fil** (mode non compact) : bordure gauche de 4 px, `button_bg` à 0.24. Elle passe en `mention_highlight_bg` si l'utilisateur y est mentionné. Padding gauche de 12 px [WP].
- **Menu de survol (MessageActions)** [CU:MessageActions] : conteneur avec padding de 2 px, bordure de 1 px center_color à 0.08, rayon 4, élévation 1. Boutons de **28 px** de haut, padding horizontal 8, 12 px semi-gras ; repos à 0.64 ; survol à fond 0.08 et texte plein. Webapp : `top: -12px; right: 0`, padding 4, bordure à 0.2 [WPM]. Boutons-icônes « small compact » de 28 × 28, icône 16 [WMX].
- **Puce de réaction** [CU:ReactionButton] : padding **4 × 6 px**, écart de 4 px, rayon 4. Fond center_color à 0.08, survol à 0.12. Emoji de 16 px. Compteur en 12 / 16, 600, à 0.56. **Ma réaction** : fond `link_color` à 0.08, contour intérieur de 1 px `link_color`, compteur `link_color`. Le compteur s'anime vers le haut ou le bas (300 ms). Webapp : 24 px de haut, min-width 28, 11 px 600, « ma réaction » en `button_bg` [WR]. Rangée de réactions : écart de 4 px entre puces ; le bouton « ajouter » n'est visible qu'au survol [CU:ReactionsRow].
- **Séparateurs** [CU:MessageSeparator] : trait de 1 px center_color à 0.12, libellé de 12 px semi-gras avec 12 px d'écart. Séparateur « nouveaux messages » : trait et libellé en `new_message_separator`.

### 4.5 Boutons

| Taille | Hauteur | Padding (vertical × horizontal) | Police | Source |
|---|---|---|---|---|
| x-small | 24 | 4 × 8 (6 en compact) | 12 / 16 | [CU:Button], [WMX] |
| small | 32 | 8 × 16 (6 × 10 en compact) | 12 / 16 | idem |
| medium | 40 | 10 × 20 (8 × 12) | 14 / 20 | idem |
| large | 48 | 12 × 24 (10 × 16) | 16 / 24 | idem |

Toutes tailles : rayon 4, police 600, écart de 8 px entre icône et texte [CU:Button], [WBT].

Variantes [CU:Button] :

- **primaire** : `button_bg`, survol avec 8 % de noir, appui avec 16 % ;
- **secondaire** : transparent, contour intérieur de 1 px `link_color`, survol à 0.08, appui à 0.16 ;
- **tertiaire** : `link_color` à 0.08, puis 0.12 et 0.16 ;
- **quaternaire** : transparent, puis 0.08 ;
- **dim** : texte center_color à 0.72, survol à 0.88 avec fond à 0.08 ;
- **désactivé** : fond center_color à 0.08, texte à 0.32 ;
- **destructif** : `error_text`.

**Boutons-icônes** : 24, 32, 40 et 48 px (compact : 20, 28, 36) ; repos à 0.64, survol à fond 0.08 et texte à 0.75, appui à fond `button_bg` 0.08 et icône `link_color` [CU:IconButton], [WBT].

### 4.6 Infobulles [CU:Tooltip]

- Fond **`#090a0b` (neutral-1200), non thématisé**, rayon 4, padding **6 × 10 px**, largeur max 200 px, élévation 3.
- Texte de 12 / 16, 600, blanc. Indication secondaire en 11 px à 75 % de blanc.
- Flèche de 4 px.
- Les raccourcis s'affichent en « keycaps » : fond blanc à 8 %, texte blanc à 72 %, rayon 4, padding 2 × 4 [CU:ShortcutTag].
- Webapp actuel : `rgba(0,0,0,.9)`, padding 4 × 8, largeur max 220 [WT].

### 4.7 Menus [CU:PopoverMenu], [CU:MenuItem]

- **Conteneur** : largeur de 212 à 496 px, padding vertical de 8 px, fond `center_bg`, bordure de 1 px center_color à 0.16, **rayon 8**, élévation 4 (5 pour un sous-menu). Ouverture en fondu combiné à une mise à l'échelle de 0.9 à 1, en 150 ms `ease-out`.
- **Élément** : padding **6 × 16 px**, texte 14 / 20. Icône de tête de 16 px dans une boîte de 24, à 0.64. Survol : center_color à 0.08. Élément actif ou coché : `button_bg` à 0.08 (0.12 au survol). Coche en `button_bg`. Raccourci affiché à droite à 0.72. Élément désactivé : contenu à 0.4.
- **Destructif** : texte `error_text` ; au survol, fond `error_text` et texte `button_color`.
- **Titre de groupe** : 11 px, 600, majuscules, à 0.64, padding 6 × 16. **Séparateur** : 1 px à 0.12, avec 8 px de marge verticale.

### 4.8 Avatars et statut [CU:UserAvatar], [CU:StatusBadge], [CC]

- Avatar rond (pilule). Pastille de statut en bas à droite, décalée de −1 px (avatars de 12 à 28 px) ou de −2 px (32 à 48 px).
- La pastille est un rond de 12, 16 ou 24 px **dont le fond vaut la couleur de la surface** (`--status-badge-bg`, soit `sidebar_bg` dans la barre latérale). Ce fond crée l'anneau de séparation.
- Couleurs : en ligne `online_indicator`, absent `away_indicator`, ne pas déranger `dnd_indicator`, **hors ligne center_color à 0.32**.

### 4.9 Modales [CU:Modal], [CU:ModalHeader], [CU:ModalFooter]

- Largeurs **600 / 704 / 832 px**. **Rayon 12**, bordure de 1 px à 0.16, élévation 6. Hauteur max : `100vh − 160px` (`− 240px` au-delà de 900 px de haut).
- **En-tête** : padding **24 × 32**, titre en Metropolis 22 / 28, 600. Sous-titre de 12 px à 0.72. Séparateur de 1 px à 0.12.
- **Corps** : padding 28 × 32. **Pied** : padding 24 × 32, boutons alignés à droite avec 8 px d'écart.
- **Entrée** : `translateY(-32px) scale(.98)` vers l'état normal, en 150 ms `ease-out`. Fond : neutral-1200 à 0.64.

### 4.10 Champ de recherche [CU:SearchInput]

- Fond `center_bg`, rayon 4.
- **Bordure dessinée en contour intérieur** (`inset 0 0 0 1px`) : 0.16 au repos, 0.24 au survol. Au focus, **double anneau de 1 px intérieur et 1 px extérieur** en `focus-ring-color` (= `button_bg`).
- Tailles : 32 px (padding 8 × 12, police de 12), 40 px (10 × 16, police de 14), 48 px. Icône de tête à 0.64, avec 8 px d'écart.
- Le libellé flotte en 10 px au-dessus du champ lors de la saisie.

---

## 5. Interaction et accessibilité

- **Anneau de focus Compass** : `outline: 2px solid var(--focus-ring-color)` avec `outline-offset: 3px`, où `--focus-ring-color` = `button_bg` [CU], [CU:Button]. Il est **réservé au clavier** (`:focus-visible`). Il apparaît par une animation de l'offset de 0 à 3 px en 150 ms, **coupée si `prefers-reduced-motion`**. Sur fond sombre (barre latérale), la couleur passe à `sidebar_text_active_border`, en contour intérieur de 2 px [CU:ChannelSidebarItem], [CU:MenuItem]. Webapp historique : `box-shadow: 0 0 0 2px sidebar_text_active_border`, retiré si le focus vient de la souris [WMX].
- **Mouvement** : chaque transition compass-ui a une variante `prefers-reduced-motion: reduce` sans transformation [CU:PopoverMenu], [CU:Modal].
- **Navigation clavier** [DOC-KB] :
  - F6 / Maj+F6 font tourner le focus entre 8 régions : liste de messages, zone de saisie, liste du panneau de droite, saisie du panneau de droite, menu d'équipe, barre latérale, en-tête de canal, recherche.
  - ↑ et ↓ passent d'un message à l'autre, ou d'une catégorie à l'autre dans la barre latérale ; Tab parcourt les actions.
  - Alt+↑ / Alt+↓ : canal précédent ou suivant. Alt+Maj+↑ / Alt+Maj+↓ : canal non lu précédent ou suivant. Ctrl+K : sélecteur rapide de canal. Ctrl+Maj+M : mentions récentes. ↑ dans une saisie vide : éditer son dernier message. Maj+↑ : répondre au dernier message.
- **Accessibilité des badges** : les compteurs portent un `aria-label` « N mentions » ; le badge de mention dans une ligne de canal est `aria-hidden`, car le statut est annoncé par un texte masqué [CU:ChannelSidebarItem] (code JS du paquet).
- **Contraste** : aucun ratio documenté publiquement. Les textes « secondaires » ne descendent pas sous 0.56 et les éléments désactivés sont à 0.32 (déduit : 0.56 reste lisible sur fond blanc pour du 12 px semi-gras, 0.32 ne vise pas la lisibilité).

---

## 6. Idées à adopter pour matterfew (par priorité)

État actuel relevé dans `src/ui/view.rs` et `src/theme.rs` : ligne de canal de 33 px en 15 px, actif à 0.16 avec bordure de 4 px ; badge de 18 px ; en-tête de catégorie de 30 px en 13.5 px gras ; texte de message en 14.5 px ; opacités ad hoc (0.06, 0.13, 0.18, 0.38, 0.45, 0.55, 0.85, 0.88…).

1. **Une échelle d'opacités unique dans `theme.rs`.** Mattermost dérive toutes ses nuances de « couleur du thème × α », avec α pris dans 0.04 / 0.08 / 0.12 / 0.16 / 0.24 / 0.32 / 0.40 / 0.56 / 0.64 / 0.72 / 0.80 [CU]. C'est ce qui garantit des thèmes personnalisés cohérents, sombres compris. À faire : ajouter des constantes `A04 … A80` et des méthodes nommées (`hover_bg()` = center à 0.04, `item_hover()` = 0.08, `border()` = 0.12, `border_strong()` = 0.16, `disabled_fg()` = 0.32, `timestamp()` = 0.56, `icon()` = 0.64), puis remplacer les valeurs hors échelle de `view.rs` (0.06→0.08, 0.13→0.12, 0.18→0.16, 0.38/0.45→0.40, 0.55→0.56, 0.85/0.88→0.80 ou 1).

2. **Ligne de canal au gabarit Compass.** C'est l'élément le plus vu, et 1 px ou 1 pt d'écart suffit à rendre le client « pas tout à fait Mattermost ». À faire : `h(32)`, texte de **14 px** (au lieu de 15) avec letter-spacing 0.02em si GPUI le permet ; nom à 0.72 et icône à 0.64 au repos ; non lu en 600 et opacité 1 ; survol `sidebar_text` à 0.08 ; actif `sidebar_text` à **0.12** (au lieu de 0.16) avec **barre de 2 px** arrondie `0 2 2 0` (au lieu de `border_l_4`) et padding gauche ramené de 18 à 16 ; sourdine à 0.4 sur icône, nom et badge ; icône de 16 px (avatar de MP de 20 px au lieu de 22) [CU:ChannelSidebarItem].

3. **Badge de mention conforme.** À faire : hauteur de 16 px (interligne 16), 11 px semi-gras (600), rayon 8 (pas `rounded_full`), padding de 6.5 / 5.5 / 4.5 px selon le nombre de chiffres, plafonné à « 99+ », min-width de 20 px pour rester proche du webapp [CU:MentionBadge], [WBG]. Badge urgent en `dnd_indicator` sur texte blanc. Sur la barre d'équipes : décalage de −3 px (au lieu de −7) et anneau de 3 px de la couleur de la barre d'équipes [WBG].

4. **Au survol d'un canal, le badge cède sa place au bouton « ⋯ ».** Compass fait un fondu de 150 ms entre badge et bouton de menu ; le badge urgent reste affiché. On n'a aujourd'hui que le clic droit, peu découvrable. À faire : bouton-icône de 24 px à 0.72, à 16 px du bord droit, visible au survol ou au focus, qui ouvre le `channel_menu` existant [CU:ChannelSidebarItem], [WSB].

5. **En-têtes de catégorie en petites capitales espacées.** Le style « 12 px / 600 / MAJUSCULES / +0.05em / 0.64 » distingue nettement une catégorie d'un canal sans surcharger la liste. À faire : `h(32)`, `text_size(12)`, `font_semibold`, majuscules à la construction de la chaîne, opacité 0.64 (au lieu de 13.5 px gras à 0.88), chevron de 16 px ; compteur et options visibles seulement au survol [WSB].

6. **Échelle typographique Compass.** À faire : `BODY_TEXT` à **14** avec interligne de 20 (au lieu de 14.5) ; horodatage en 12 / 16 à 0.56 ; noms en 14 semi-gras ; titres (en-tête de canal, modales) en 16 ou 22 px semi-gras. Ajouter des constantes `FS_25 … FS_1000` et `LH_*` dans `view.rs` [CU]. Open Sans et Metropolis ne s'imposent pas : la police native Windows reste un choix défendable (déduit). Si on veut l'identité Mattermost, Metropolis peut servir aux seuls titres.

7. **Une seule échelle de rayons : 4 / 8 / 12 / pilule.** Aujourd'hui on mélange 3, 5, 6, 8 et 13 px. À faire : 4 pour les boutons, puces, éléments de menu, champs et lignes ; 8 pour les menus, popovers et la boîte de recherche ; 12 pour les modales et la carte centrale ; pilule pour les badges et avatars [CU], [WV].

8. **Ombres à 6 niveaux.** À faire : une fonction `elevation(n)` dans `theme.rs` qui renvoie les valeurs de §1.3, avec 0.32 d'opacité en thème sombre. Usage : niveau 1 pour le menu de survol, 3 pour les infobulles, 4 pour les menus, 5 pour les sous-menus, 6 pour les modales et la fenêtre de fil (440 px) [WV], [CC].

9. **Barre d'actions au survol d'un message.** À faire : carte flottante en `top: -12px` à droite, avec padding de 2 px, bordure de 1 px à 0.08, rayon 4 et élévation 1. Elle contient des boutons-icônes de 28 × 28 (icône de 16 px à 0.64, survol à fond 0.08 et icône à 0.75) : réaction rapide, répondre, « ⋯ » [CU:MessageActions], [WPM]. Le survol de la ligne reste à 0.04, déjà en place.

10. **Puces de réaction Compass.** À faire : padding 4 × 6, rayon 4, 4 px entre puces, fond à 0.08 (survol 0.12), emoji de 16 px, compteur en 12 px semi-gras à 0.56. **Ma réaction** : fond `link` à 0.08, contour intérieur de 1 px `link`, compteur en `link`. Notre version actuelle utilise `button_bg` avec une bordure ; Compass passe à `link_color`. Bouton « ajouter une réaction » visible au survol seulement [CU:ReactionButton], [CU:ReactionsRow].

11. **Abandonner la teinte sur ses propres messages, ou la rendre optionnelle.** Mattermost ne colore pas les messages de l'utilisateur. Il réserve les fonds aux états qui ont un sens : permalien (mention_highlight à 0.5, estompé après 5 s), épinglé ou enregistré (mélange opaque à 24 %), édition (`button_bg` à 0.08). À faire : retirer `button_bg.opacity(0.05)` et `border_l_2` sur `mine`, ou les placer derrière un réglage ; mettre la mise en évidence à 0.5 avec estompage, et l'épinglage au mélange à 24 % [WP], [WU].

12. **Anneau de focus clavier unique.** À faire : contour de 2 px `button_bg` décalé de 3 px (sur fond sombre : 2 px intérieur `sidebar_text_active_border`), seulement en navigation clavier. GPUI distingue `focus` de `focus_visible` : n'afficher l'anneau qu'avec ce dernier (déduit). Ajouter un cycle F6 entre les régions (liste, saisie, fil, équipes, barre latérale, en-tête, recherche), Alt+↑/↓ et Alt+Maj+↑/↓ si ces raccourcis manquent [CU], [DOC-KB].

13. **Menus et infobulles au gabarit Compass.** Menus : largeur min 212 px (au lieu de 220 et 240), padding vertical de 8, éléments de 6 × 16 px en 14 / 20, icônes de 16 px à 0.64, survol à 0.08, destructif sur fond `error` au survol, titres de groupe de 11 px en majuscules, séparateurs de 1 px à 0.12. Infobulles : fond `#090a0b` fixe, padding 6 × 10, 12 px semi-gras blanc, largeur max 200, raccourcis en keycaps (blanc 8 % / 72 %) [CU:PopoverMenu], [CU:MenuItem], [CU:Tooltip], [CU:ShortcutTag].

14. **Boutons à 4 tailles fixes : 24 / 32 / 40 / 48.** Paddings de 8 / 16 / 20 / 24, police de 12 / 12 / 14 / 16, rayon 4, graisse 600. Primaire : 8 % de noir au survol, 16 % à l'appui. Secondaire : contour intérieur `link`. Tertiaire : `link` à 0.08 / 0.12 / 0.16. Désactivé : fond 0.08, texte 0.32. À faire : un helper `mm_button(size, emphasis)` pour ne plus régler chaque bouton à la main (bouton « + » de 32 px, rayon 16…) [CU:Button].

15. **Police d'icônes Compass (optionnel).** `@mattermost/compass-icons` (licence MIT d'après son `package.json`, v0.1.65) fournit `compass-icons.ttf` avec les mêmes glyphes que le webapp : canal, cadenas, globe, crayon de brouillon, statuts. GPUI peut charger une TTF et afficher un glyphe par son point de code (déduit : reste à vérifier sur la table `config.json` du paquet). Sources : https://www.npmjs.com/package/@mattermost/compass-icons, https://github.com/mattermost/compass-icons.

Bonus (petits ajustements) :

- Barre d'équipes de 65 px au lieu de 64 [WSB].
- Carte centrale arrondie : `margin 0 4 4`, rayon 12, bordure à 0.08, élévation 1 [WS].
- Pastilles de statut sur fond de la surface : on le fait déjà avec `ring` ; le hors-ligne passe à center_color à 0.32 [CU:StatusBadge].
- Séparateur « nouveaux messages » en 12 px semi-gras, trait de 1 px [CU:MessageSeparator].
