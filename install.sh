#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
install_dir=${HOME}/.local/bin

mkdir -p "$install_dir"
chmod +x "$project_dir/bin/ui-notes" "$project_dir/bin/ui-notes-panel" "$project_dir/share/ui-notes-theme-hook"
ln -sf "$project_dir/bin/ui-notes" "$install_dir/ui-notes"
ln -sf "$project_dir/bin/ui-notes-panel" "$install_dir/ui-notes-panel"

if command -v omarchy >/dev/null 2>&1; then
  omarchy hook install theme-set "$project_dir/share/ui-notes-theme-hook"
fi
