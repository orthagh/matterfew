# Changelog

Notable changes in each version. Format:
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versions:
[SemVer](https://semver.org/).

To release: move the content of "Unreleased" under a
`## [x.y.z] - YYYY-MM-DD` heading, set the same version in `Cargo.toml`,
then push the `vx.y.z` tag. The GitHub release reuses that section.

## [Unreleased]

### Changed

- New typography, the same on every desktop: Nunito Sans for text and
  Bricolage Grotesque for titles, both embedded.
- Channel icons are a symbol (public, private, group) on a light tint of the
  channel's colour, instead of initials on a saturated gradient.

### Fixed

- Webhook messages (GitLab…) show the integration's icon instead of the
  picture of whoever created the webhook.

## [0.4.0] - 2026-11-25

### Added

- Redesigned message box: a rounded card with only "+" (attach, mention,
  formatting bar, preview, Markdown help), emoji and a round send button
  that lights up when there is something to send; formatting appears above
  selected text.
- Spelling suggestions and "Add to dictionary" show in a small bubble when
  the cursor is on an underlined word, instead of a bar in the message box.
- The Unreads section of the sidebar can be collapsed, like a category.
- Linux: text selected with the mouse is pasted by a middle click, in the
  message box and the search field too.
- Clicking the time of a message shows it in context, as in the web client:
  from a thread, in the channel; a reply in the channel opens its thread.
- Right-clicking a link offers to copy or open it.
- Pasting a web address over selected text turns it into a Markdown link.
- Spell checking accepts English words as well as the interface language,
  and suggests corrections from both.
- Spell checking on Linux and macOS, with the system Hunspell dictionaries
  or the bundled French and English ones (LibreOffice, MPL 2.0 / SCOWL).
- A sent message shows at once, faded until the server confirms it; if
  sending fails, the reason and Retry / Edit / Discard appear under it.
- Dragging files over the channel or a thread shows where to drop them.
- MCP server (beta, read-only, off by default): an assistant such as Claude
  Code can search and read messages through Matterfew. Turn it on in
  Settings > Advanced.

### Fixed

- Linux: notifications of a read channel are now really closed (GNOME only
  lets the connection that sent a notification close it).
- Flatpak: the launcher badge count was dropped by the sandbox.
- Linux (Wayland): clicking a notification brings the window forward.
- The profile beside a direct message lines up with the channel header.
- A server error shown at startup now clears once the connection is back.
- A right click on a link no longer opens it.
- Spelling marks from a previous text (sent message, other channel's draft)
  no longer linger in the message box.
- Windows spell checking no longer flags hyphenated words whose parts are
  correct ("dites-moi").
- macOS: formatting shortcuts use Cmd (⌘B, ⌘I…) instead of Ctrl, and the
  tooltips say so.
- Spell checking waits for a pause in typing instead of underlining words
  still being typed; errors already marked stay in place meanwhile.
- Shift+Enter inserted a line break and then sent the message; it now only
  goes to the next line. Alt+Enter goes to the next line too, and
  Ctrl+Enter closes an open code block before sending, as in the web client.
- Flatpak: files dragged from the home folder can be attached (the app now
  has read-only access to it; flatpak asks to confirm this on update). A
  dropped file still out of reach shows a message pointing to the paperclip.
- No more "server not responding" error for a quiet connection or after the
  computer wakes up; a dropped connection only shows the offline banner
  while it reconnects.
- Scrolling down with the mouse wheel when already at the bottom of a long
  channel no longer shows the button to jump to new messages.

## [0.3.0] - 2026-11-24

### Added

- Mouse back and forward buttons move through the channel history.
- Clicking the text of a message opens its thread, unless "Click to open
  threads" is turned off in the display settings.
- With threads shown in the channel, replies sit on a panel along a thread
  line, and a run of replies away from its thread starts with the message
  it answers.

### Changed

- Own messages keep their tint but no longer have a vertical bar, which now
  only marks threads.
- Bot notifications show their time right after the title.
- GitHub issue and pull request notifications are a single muted line where
  only the title stands out.
- The "Threads" entry of the sidebar only shows with collapsed threads, as
  in the web client.

### Fixed

- Enter in the server field of the login screen starts the login.
- Direct and group messages closed in another client, or never opened, no
  longer show in the sidebar.
- `diff` and `patch` code blocks had no colours.
- Quoted message previews keep their Markdown formatting.
- Linux: bold text and unread channels were not bold when the system font is
  a variable font (Ubuntu Sans, Cantarell).
- Linux: notifications of a channel are closed once it is read, so the
  launcher badge no longer lingers.
- Bot messages other than GitHub or GitLab notifications no longer get a
  colour guessed from their wording.
- Channels read or marked unread in another client (web, mobile) now update here.
- Some emoji names (`:unicorn_face:`, `:male-teacher:`) showed as text.
- Linux: the launcher badge follows server events instead of redraws, and the
  urgency hint no longer resets its count.

## [0.2.1] - 2026-11-23

### Added

- Settings window: notifications (with a choice of sounds), display,
  sidebar and advanced options.
- Keywords highlighted in messages, like mentions.
- Do not disturb with a duration.
- Sidebar filter for unread channels only.
- Close a direct or group message.
- Join and leave teams, edit team settings.
- File uploads: progress, cancel, server limits shown before sending.
- Message length counter and limit.
- Formatting help in the composer.
- Right panel can be widened.
- Rich editor (beta): formatting shown while typing.
- Syntax highlighting in code blocks.
- Automatic updates: signed download and restart for the Windows installer,
  the macOS app and the AppImage; flatpak repository on GitHub Pages; a
  notice with a link for snap and .deb.

### Fixed

- Snap: crash at startup on AMD GPUs; login page that did not load.
- Linux: "Log in via Mattermost" failed under Wayland.
- Linux: no title bar under GNOME on Wayland.
- Flatpak: built on the GNOME 51 runtime (47 reached end of life).

## [0.2.0] - 2026-11-22

### Added

- Server links and `~channel` open in the app.
- Paste images and files into a message.
- New direct and group messages; browse public and archived channels.
- Threads: follow or unfollow, team-wide Threads view.
- Custom status, message reminders, forwarding.
- Per-channel notification preferences.
- Channel settings, bookmarks and member roles; rename and delete categories.
- Search in a channel's files and links; PDF preview.
- Retry, edit or discard a message that failed to send.
- Confirmation before @channel, @all and @here; add a mentioned person to
  the channel.
- Direct messages: warning when the other person is away or in do not
  disturb.
- Recent emojis, "who reacted" tooltips, new message toasts.
- Banner when the connection to the server is lost.
- More keyboard shortcuts (list with Ctrl+/ or F1); send with Ctrl+Enter
  when the server preference asks for it.
- Accessibility: screen reader labels, F6 to move between input areas.

### Changed

- The app is now called **Matterfew** (`orthagh/matterfew`); settings, cache
  and sign-in saved under the old name are picked up on first launch.
- Sidebar and status menu polish.

## [0.1.0] - 2026-11-21

First public release.

### Added

- Sign-in through the server page (GitLab, password, two-factor
  authentication), by email or by token, stored in the system keyring.
- Mattermost-style sidebar: categories, sorting, unreads, favorites, mute,
  drag and drop; create channels and invite people.
- Messages: Markdown, emojis, reactions, threads, pinned and saved messages,
  attachments, link previews, integrations.
- Search with filters and history, recent mentions, Ctrl+K switcher.
- Profile and team cards, channel info panel, member list.
- Composer: formatting, preview, autocomplete (people, channels, emojis,
  slash commands).
- Server theme and a layout that adapts to narrow windows.
- Windows: notifications, taskbar badge, tray icon, single instance, spell
  check.
- Local message cache and a WebSocket that catches up after a disconnect.
- Interface in English, French and Brazilian Portuguese (`MM_LANG` forces
  one).
- Installers for Windows, macOS and Linux (snap and flatpak).
