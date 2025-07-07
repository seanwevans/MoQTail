#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
HOOKS_SRC="$REPO_ROOT/scripts/hooks"
GIT_HOOKS_DIR="$REPO_ROOT/.git/hooks"

mkdir -p "$GIT_HOOKS_DIR"

for src in "$HOOKS_SRC"/*; do
  [ -f "$src" ] || continue
  hook_name="$(basename "$src")"
  dest="$GIT_HOOKS_DIR/$hook_name"
  rm -f "$dest"
  if ln -s "$src" "$dest" 2>/dev/null; then
    echo "Symlinked $hook_name"
  else
    cp "$src" "$dest"
    echo "Copied $hook_name"
  fi
  chmod +x "$dest"
 done
