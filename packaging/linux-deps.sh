#!/usr/bin/env sh
# Bibliothèques nécessaires pour compiler sous Debian/Ubuntu :
# GPUI (Wayland, X11, Vulkan, polices), la fenêtre de connexion (WebKitGTK),
# le trousseau (D-Bus) et TLS (OpenSSL).
set -eu
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential pkg-config clang \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev \
  libx11-dev libx11-xcb-dev libxcb1-dev libxi-dev libxcursor-dev libxrandr-dev \
  libfontconfig-dev libfreetype-dev libasound2-dev libzstd-dev \
  libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libxdo-dev \
  libdbus-1-dev libssl-dev
