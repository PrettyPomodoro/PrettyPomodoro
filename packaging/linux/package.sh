#!/usr/bin/env bash
# Builds the Linux binary and packs it with its icon and desktop entry into dist/.
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo build --release --locked

name=Pomodoro-linux-x64
rm -rf "dist/$name" "dist/$name.tar.gz"
mkdir -p "dist/$name"
cp target/release/pomodoro "dist/$name/pomodoro"
cp assets/icon.png "dist/$name/pomodoro.png"
cp packaging/linux/pomodoro.desktop "dist/$name/pomodoro.desktop"
cat > "dist/$name/install.sh" <<'INSTALL'
#!/usr/bin/env bash
# Installs Pomodoro for the current user.
set -euo pipefail
cd "$(dirname "$0")"
install -Dm755 pomodoro "$HOME/.local/bin/pomodoro"
install -Dm644 pomodoro.png "$HOME/.local/share/icons/hicolor/256x256/apps/pomodoro.png"
install -Dm644 pomodoro.desktop "$HOME/.local/share/applications/pomodoro.desktop"
echo "Installed to ~/.local/bin/pomodoro"
INSTALL
chmod +x "dist/$name/install.sh"
tar -C dist -czf "dist/$name.tar.gz" "$name"
echo "dist/$name.tar.gz"
