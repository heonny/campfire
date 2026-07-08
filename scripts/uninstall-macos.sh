#!/usr/bin/env bash
#
# Remove Campfire's saved data for a clean uninstall.
#
# Campfire keeps only two files — your server list (servers.toml) and its
# running-state (running.json) — and on macOS both live in one folder. This
# deletes that folder. It does NOT remove the app itself: drag Campfire.app
# to the Trash separately.
#
# Usage:
#   ./scripts/uninstall-macos.sh          # shows the path and asks first
#   ./scripts/uninstall-macos.sh --yes    # delete without the prompt
#
# macOS only.

set -euo pipefail

# Guard against an empty $HOME expanding the target to a system path.
: "${HOME:?HOME is not set}"

# The `directories` crate maps both the config dir and the data dir to the same
# Application Support folder on macOS, so there is a single directory to remove.
DATA_DIR="$HOME/Library/Application Support/com.heonny.campfire"

YES=0
for arg in "$@"; do
  case "$arg" in
    --yes|-y) YES=1 ;;
    *) echo "error: unknown argument: $arg" >&2; exit 1 ;;
  esac
done

[[ "$(uname)" == "Darwin" ]] || { echo "error: macOS only" >&2; exit 1; }

if [[ ! -e "$DATA_DIR" ]]; then
  echo "Nothing to remove — $DATA_DIR does not exist."
  exit 0
fi

echo "This permanently deletes Campfire's saved data (server list + state):"
echo "  $DATA_DIR"
echo
echo "The app itself is left alone — move Campfire.app to the Trash separately."
echo

if [[ "$YES" != 1 ]]; then
  read -r -p "Delete it? [y/N] " reply
  case "$reply" in
    [yY] | [yY][eE][sS]) ;;
    *) echo "Aborted."; exit 0 ;;
  esac
fi

rm -rf "$DATA_DIR"
echo "==> removed $DATA_DIR"
