#!/bin/bash
set -euo pipefail

plugin_id=cordrogue.ui-notes
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
plugin_dir=${UI_NOTES_PLUGIN_DIR:-$HOME/.config/omarchy/plugins/$plugin_id}
install_dir=${UI_NOTES_INSTALL_DIR:-$HOME/.local/bin}
enable=true
case "${1:-}" in
  --no-enable) enable=false ;;
  '') ;;
  *) echo "Usage: ./install.sh [--no-enable]" >&2; exit 1 ;;
esac
fail() { echo "ui-notes install: $*" >&2; exit 1; }
for program in cargo omarchy sha256sum jq; do
  command -v "$program" >/dev/null || fail "missing dependency: $program"
done
[[ ! -L $plugin_dir ]] || fail "plugin directory must not be a symlink"
plugin_dir=$(realpath -m -- "$plugin_dir")
install_dir=$(realpath -m -- "$install_dir")
[[ $plugin_dir != / && $plugin_dir != "$HOME" ]] || fail "unsafe plugin directory"

# Validate source before building or changing the installed copy.
omarchy plugin validate "$project_dir"
if [[ $project_dir != "$plugin_dir" && -e $plugin_dir ]]; then
  [[ -f $plugin_dir/.ui-notes-install && $(cat "$plugin_dir/.ui-notes-install") == "$plugin_id" ]] \
    || fail "existing plugin directory is not managed by this installer: $plugin_dir"
fi
if [[ -f $plugin_dir/.ui-notes-install ]]; then
  omarchy plugin validate "$plugin_dir"
  [[ -f $plugin_dir/.ui-notes-files ]] || fail "missing installed file checksums"
  (cd "$plugin_dir" && sha256sum --check --quiet .ui-notes-files) \
    || fail "installed files were edited or removed; save those changes before reinstalling"
fi
for name in ui-notes ui-notes-panel; do
  link=$install_dir/$name
  if [[ -e $link || -L $link ]]; then
    [[ -L $link ]] || fail "refusing to replace a file: $link"
    destination=$(readlink -- "$link")
    [[ $destination == "$project_dir/bin/$name" || $destination == "$plugin_dir/bin/$name" \
      || $destination == "$project_dir/target/release/$name" ]] \
      || fail "refusing to replace an unrelated link: $link"
  fi
done

# Flush a loaded panel before replacing its code. Staging skips desktop IPC.
if $enable; then
  runtime_dir=${XDG_RUNTIME_DIR:-/tmp}
  if [[ -n ${XDG_RUNTIME_DIR:-} ]]; then runtime_dir=$runtime_dir/ui-notes; else runtime_dir=$runtime_dir/ui-notes-$(id -u); fi
  if [[ -S $runtime_dir/panel.sock ]]; then
    current_cli=$install_dir/ui-notes
    [[ -x $current_cli ]] || fail "panel is running but its installed CLI is unavailable"
    if flush_reply=$("$current_cli" panel quit 2>&1); then
      jq -e '.ok == true' <<< "$flush_reply" >/dev/null \
        || fail "panel could not save and close; installation stopped: $flush_reply"
    elif [[ $flush_reply == 'panel not running: Connection refused (os error 111)' \
      || $flush_reply == 'panel not running: No such file or directory (os error 2)' ]]; then
      : # A stale or vanished socket has no running panel to flush.
    else
      fail "panel could not save and close; installation stopped: $flush_reply"
    fi
  fi
fi

# Keep wrappers independent of a caller's CARGO_TARGET_DIR.
cargo build --manifest-path "$project_dir/Cargo.toml" --locked --release --target-dir "$project_dir/target"
if [[ $project_dir != "$plugin_dir" ]]; then
  stage=$(mktemp -d)
  trap 'rm -rf -- "$stage"' EXIT
  for item in manifest.json Plugin.qml qml bin share Cargo.toml Cargo.lock src tests .gitignore README.md CONTRACT.md PERFORMANCE.md benchmarks docs install.sh uninstall.sh; do
    [[ ! -e $project_dir/$item ]] || cp -R -- "$project_dir/$item" "$stage/$item"
  done
  [[ ! -f $project_dir/LICENSE ]] || cp -- "$project_dir/LICENSE" "$stage/LICENSE"
  mkdir -p "$stage/target/release"
  cp -- "$project_dir/target/release/ui-notes" "$project_dir/target/release/ui-notes-panel" "$stage/target/release/"
  omarchy plugin validate "$stage"
  (cd "$stage" && find . -type f ! -path './.ui-notes-files' -print0 | sort -z | xargs -0 sha256sum) \
    > "$stage/.ui-notes-files"
  mkdir -p "$plugin_dir"
  declare -A staged_files=()
  while IFS= read -r record; do
    file=${record#*  }
    staged_files["$file"]=1
  done < "$stage/.ui-notes-files"
  # Remove files the previous release installed that this one no longer ships.
  if [[ -f $plugin_dir/.ui-notes-files ]]; then
    while IFS= read -r record; do
      checksum=${record%% *}
      file=${record#*  }
      [[ $checksum =~ ^[0-9a-f]{64}$ && $file == ./* && $file != *'..'* ]] || continue
      [[ -z ${staged_files[$file]+x} ]] || continue
      installed_file=$plugin_dir/${file#./}
      if [[ -f $installed_file && ! -L $installed_file ]]; then
        actual_checksum=$(sha256sum -- "$installed_file")
        if [[ ${actual_checksum%% *} == "$checksum" ]]; then
          rm -- "$installed_file"
          parent=${installed_file%/*}
          while [[ $parent != "$plugin_dir" ]]; do
            rmdir -- "$parent" 2>/dev/null || break
            parent=${parent%/*}
          done
        else
          echo "Preserved modified file: $installed_file"
        fi
      fi
    done < "$plugin_dir/.ui-notes-files"
  fi
  # Replace inodes: the keep-loaded backend may still execute the old binary.
  cp -R --remove-destination -- "$stage/." "$plugin_dir/"
  printf '%s\n' "$plugin_id" > "$plugin_dir/.ui-notes-install"
fi
omarchy plugin validate "$plugin_dir"
mkdir -p "$install_dir"
for name in ui-notes ui-notes-panel; do
  ln -sfn -- "$plugin_dir/bin/$name" "$install_dir/$name"
done
if $enable; then
  omarchy-shell shell rescanPlugins >/dev/null
  discovered=false
  for ((attempt = 0; attempt < 30; attempt++)); do
    if registry=$(OMARCHY_SHELL_IPC_TIMEOUT=1 omarchy-shell shell listPlugins) \
      && jq -e --arg id "$plugin_id" 'any(.[]; .id == $id)' <<< "$registry" >/dev/null; then
      discovered=true
      break
    fi
    sleep 0.1
  done
  $discovered || fail "plugin was copied but not discovered; inspect the shell log, then rescan and enable $plugin_id"
  omarchy plugin enable "$plugin_id"
else
  echo "Built and installed. Enable with: omarchy-shell shell rescanPlugins && omarchy plugin enable $plugin_id"
fi
echo "Installed UI Notes at $plugin_dir. Run: ui-notes open"
