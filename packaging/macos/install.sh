#!/usr/bin/env bash
# Builds PrettyPomodoro.app for this Mac, installs it into /Applications and opens it.
set -euo pipefail
cd "$(dirname "$0")/../.."

packaging/macos/bundle.sh --native

installed=/Applications/PrettyPomodoro.app
pkill -f "$installed/Contents/MacOS/prettypomodoro" 2> /dev/null || true
rm -rf "$installed"
ditto dist/PrettyPomodoro.app "$installed"
open "$installed"
echo "Installed $installed"
