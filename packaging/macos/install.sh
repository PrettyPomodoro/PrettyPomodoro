#!/usr/bin/env bash
# Builds PrettyPomodoro.app for this Mac, installs it into /Applications and opens it.
set -euo pipefail
cd "$(dirname "$0")/../.."

packaging/macos/bundle.sh --native

installed=/Applications/PrettyPomodoro.app
# Quit the running copy and wait for it to go, or `open` can fail to relaunch.
pkill -f "$installed/Contents/MacOS/prettypomodoro" 2> /dev/null || true
for _ in 1 2 3 4 5 6 7 8 9 10; do
    pgrep -f "$installed/Contents/MacOS/prettypomodoro" > /dev/null || break
    sleep 0.2
done
rm -rf "$installed"
ditto dist/PrettyPomodoro.app "$installed"
open "$installed" || { sleep 1; open "$installed"; }
echo "Installed $installed"
