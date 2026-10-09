#!/usr/bin/env bash
# Builds Pomodoro.app for this Mac, installs it into /Applications and opens it.
set -euo pipefail
cd "$(dirname "$0")/../.."

packaging/macos/bundle.sh --native

installed=/Applications/Pomodoro.app
pkill -f "$installed/Contents/MacOS/pomodoro" 2> /dev/null || true
rm -rf "$installed"
ditto dist/Pomodoro.app "$installed"
open "$installed"
echo "Installed $installed"
