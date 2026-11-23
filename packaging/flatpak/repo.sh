#!/bin/sh
# Dépôt flatpak publié sur GitHub Pages : `flatpak update` et les logithèques
# (GNOME Logiciels, Discover) y trouvent les nouvelles versions.
#
#   sh packaging/flatpak/repo.sh PAQUET.flatpak IDENTIFIANT_GPG URL_DU_SITE SORTIE
#
# Le dépôt est reconstruit à chaque release avec la seule dernière version :
# les clients installés récupèrent simplement le nouveau commit.
set -eu

bundle=$1
key=$2
site=$3
out=$4
app=io.github.orthagh.Matterfew

mkdir -p "$out"
ostree init --mode=archive-z2 --repo="$out/repo"
flatpak build-import-bundle --gpg-sign="$key" "$out/repo" "$bundle"
flatpak build-update-repo --gpg-sign="$key" --generate-static-deltas \
    --title=Matterfew --default-branch=master "$out/repo"

gpgkey=$(gpg --export "$key" | base64 -w0)

cat > "$out/matterfew.flatpakrepo" <<EOF
[Flatpak Repo]
Title=Matterfew
Url=$site/repo/
Homepage=https://github.com/orthagh/matterfew
Comment=Native Mattermost client
GPGKey=$gpgkey
EOF

cat > "$out/$app.flatpakref" <<EOF
[Flatpak Ref]
Name=$app
Branch=master
Title=Matterfew
Url=$site/repo/
SuggestRemoteName=matterfew
Homepage=https://github.com/orthagh/matterfew
IsRuntime=false
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
GPGKey=$gpgkey
EOF

cat > "$out/index.html" <<EOF
<!doctype html>
<meta charset="utf-8">
<title>Matterfew flatpak</title>
<p>Install Matterfew with flatpak:</p>
<pre>flatpak install --from $site/$app.flatpakref</pre>
<p>Updates then come through <code>flatpak update</code> or your software center.</p>
EOF
touch "$out/.nojekyll"
