#!/usr/bin/env bash
# Prints the package version from Cargo.toml.
set -euo pipefail
sed -n 's/^version = "\(.*\)"/\1/p' "$(dirname "$0")/../Cargo.toml" | head -n 1
