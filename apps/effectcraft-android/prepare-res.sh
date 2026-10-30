#!/usr/bin/env sh
# Generates apps/effectcraft-android/res (the launcher icons cargo-apk packs) from the
# repository's own app icon. Nothing is committed: `res/` is ignored, so no asset sidecar is needed.
# Run it once before `cargo apk build`; the CI workflow does.
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$here/../.." && pwd)
icon="$repo/assets/app-icon/hicolor/256x256/apps/ai.storyteller.effectcraft.png"

if [ ! -f "$icon" ]; then
    echo "prepare-res: app icon not found: $icon" >&2
    exit 1
fi

# Android scales one image to every density; a 256 px source is sharp up to xxhdpi.
for density in mdpi hdpi xhdpi xxhdpi xxxhdpi; do
    mkdir -p "$here/res/mipmap-$density"
    cp "$icon" "$here/res/mipmap-$density/ic_launcher.png"
done
echo "prepare-res: wrote $here/res"
