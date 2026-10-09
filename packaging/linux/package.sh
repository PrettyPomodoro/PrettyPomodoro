#!/usr/bin/env bash
# Builds the Linux binary and packs it with its icon and desktop entry into dist/.
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo build --release --locked

name=PrettyPomodoro-linux-x64
rm -rf "dist/$name" "dist/$name.tar.gz"
mkdir -p "dist/$name"
cp target/release/prettypomodoro "dist/$name/prettypomodoro"
cp assets/icon.png "dist/$name/prettypomodoro.png"
cp packaging/linux/prettypomodoro.desktop "dist/$name/prettypomodoro.desktop"
cat > "dist/$name/install.sh" <<'INSTALL'
#!/usr/bin/env bash
# Installs PrettyPomodoro for the current user.
set -euo pipefail
cd "$(dirname "$0")"
install -Dm755 prettypomodoro "$HOME/.local/bin/prettypomodoro"
install -Dm644 prettypomodoro.png "$HOME/.local/share/icons/hicolor/256x256/apps/prettypomodoro.png"
install -Dm644 prettypomodoro.desktop "$HOME/.local/share/applications/prettypomodoro.desktop"
echo "Installed to ~/.local/bin/prettypomodoro"
INSTALL
chmod +x "dist/$name/install.sh"
tar -C dist -czf "dist/$name.tar.gz" "$name"
echo "dist/$name.tar.gz"
