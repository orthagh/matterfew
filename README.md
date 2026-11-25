# Matterfew

> [!WARNING]
> This is a personal project, shaped around my own needs and my team's
> Mattermost server. It is not meant to be a general-purpose client: some
> features may be missing, and others may behave differently from the
> official apps. Use it at your own risk.
>
> Matterfew is an unofficial client, not affiliated with or endorsed by
> Mattermost, Inc. "Mattermost" is a trademark of Mattermost, Inc.

A native, lightweight desktop client for [Mattermost](https://mattermost.com),
written in Rust with [GPUI](https://gpui.rs), the UI engine behind Zed. No
Electron, no embedded browser.

![The client, connected to a demo server](docs/capture.png)

## What it does

- Everyday Mattermost: your server's channels and categories, threads,
  reactions, attachments, search, mentions, direct messages.
- A fast interface that follows your Mattermost theme.
- Bot notifications (GitHub, GitLab…) condensed to one line each.
- Native notifications and badges, sign-in kept in the system keychain.

More details (in French) in [docs/FONCTIONNALITES.md](docs/FONCTIONNALITES.md).

## Install

Download the latest version from
[Releases](https://github.com/orthagh/matterfew/releases):
Windows installer, macOS disk image, snap or flatpak for Linux.

Releases are not signed yet: Windows and macOS ask for confirmation on first
launch.

### Updates

The Windows installer, the macOS app and the AppImage update themselves: a
banner offers each new version, and one click downloads it, checks its
signature, installs it and restarts the app.

With flatpak, install from the Matterfew repository once; `flatpak update`
and your software center then keep it up to date:

```bash
flatpak install --from https://orthagh.github.io/matterfew/io.github.orthagh.Matterfew.flatpakref
```

Snap and `.deb` users get a notice in the app with a link to the new release.

## MCP server (beta)

Matterfew can let an assistant such as Claude Code search and read your
messages through its own session: turn on **Settings > Advanced > MCP
server**, then paste the command it copies, for example:

```bash
claude mcp add --transport http matterfew http://127.0.0.1:41814/mcp --header "Authorization: Bearer <token>"
```

The server only listens on this computer and requires that token. It is
read-only: search, threads, channel history, channel list, user profiles,
and what is open in Matterfew. It cannot post, react or mark anything as
read.

## Try it without a server

```bash
cargo run -- --demo
```

## Build

You need [Rust](https://rustup.rs) (stable). On Linux, a few system libraries
are required as well: see `packaging/linux-deps.sh`.

```bash
cargo build --release
cargo test
```

Changes for each version are listed in the [CHANGELOG](CHANGELOG.md).

## License

[MIT](LICENSE)
