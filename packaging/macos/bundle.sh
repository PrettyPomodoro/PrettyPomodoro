#!/usr/bin/env bash
# Builds a universal (Apple Silicon + Intel) Pomodoro.app and zips it into dist/.
# With --native it builds for this Mac's architecture only, which is faster.
set -euo pipefail
cd "$(dirname "$0")/../.."

version=$(packaging/version.sh)
if [[ "${1:-}" == "--native" ]]; then
    targets=("$(rustc -vV | sed -n 's/^host: //p')")
else
    targets=(aarch64-apple-darwin x86_64-apple-darwin)
fi
binaries=()
for target in "${targets[@]}"; do
    cargo build --release --locked --target "$target"
    binaries+=("target/$target/release/pomodoro")
done

app=dist/Pomodoro.app
rm -rf dist/Pomodoro.app dist/Pomodoro-macos-universal.zip
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/pomodoro" "${binaries[@]}"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"

# AppIcon.icns from the padded icon, at every size the Dock and Finder ask for.
iconset=$(mktemp -d)/AppIcon.iconset
mkdir -p "$iconset"
for size in 16 32 128 256; do
    sips -z "$size" "$size" assets/icon-macos.png --out "$iconset/icon_${size}x${size}.png" > /dev/null
    sips -z "$((size * 2))" "$((size * 2))" assets/icon-macos.png --out "$iconset/icon_${size}x${size}@2x.png" > /dev/null
done
cp assets/icon-macos.png "$iconset/icon_512x512.png"
iconutil -c icns -o "$app/Contents/Resources/AppIcon.icns" "$iconset"

# Ad-hoc signature: required for the merged binary to run on Apple Silicon.
codesign --force --deep --sign - "$app"

if [[ ${#targets[@]} -gt 1 ]]; then
    ditto -c -k --keepParent "$app" dist/Pomodoro-macos-universal.zip
    echo "dist/Pomodoro-macos-universal.zip"
fi
