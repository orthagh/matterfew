# Changelog

Notable changes in each version. Format:
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versions:
[SemVer](https://semver.org/).

To release: move the content of "Unreleased" under a
`## [x.y.z] - YYYY-MM-DD` heading, set the same version in `Cargo.toml`,
then push the `vx.y.z` tag. The GitHub release reuses that section.

## [Unreleased]

### Fixed

- Channels read or marked unread in another client (web, mobile) now update here.
- Some emoji names (`:unicorn_face:`, `:male-teacher:`) showed as text.

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
