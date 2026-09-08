#!/bin/bash
set -euo pipefail

plugin_id=cordrogue.ui-notes
plugin_dir=${UI_NOTES_PLUGIN_DIR:-$HOME/.config/omarchy/plugins/$plugin_id}
install_dir=${UI_NOTES_INSTALL_DIR:-$HOME/.local/bin}
disable=true
case "${1:-}" in
  --no-disable) disable=false ;;
  '') ;;
  *) echo "Usage: ./uninstall.sh [--no-disable]" >&2; exit 1 ;;
esac
[[ ! -L $plugin_dir ]] || { echo "Refusing a symlink plugin directory" >&2; exit 1; }
plugin_dir=$(realpath -m -- "$plugin_dir")
install_dir=$(realpath -m -- "$install_dir")
if $disable && [[ -d $plugin_dir ]]; then
  runtime_dir=${XDG_RUNTIME_DIR:-/tmp}
  if [[ -n ${XDG_RUNTIME_DIR:-} ]]; then runtime_dir=$runtime_dir/ui-notes; else runtime_dir=$runtime_dir/ui-notes-$(id -u); fi
  if [[ -S $runtime_dir/panel.sock ]]; then
    if flush_reply=$("$install_dir/ui-notes" panel quit 2>&1); then
      jq -e '.ok == true' <<< "$flush_reply" >/dev/null || {
        echo "Panel could not save and close; removal stopped: $flush_reply" >&2
        exit 1
      }
    elif [[ $flush_reply == 'panel not running: Connection refused (os error 111)' \
      || $flush_reply == 'panel not running: No such file or directory (os error 2)' ]]; then
      : # A stale or vanished socket has no running panel to flush.
    else
      echo "Panel could not save and close; removal stopped: $flush_reply" >&2
      exit 1
    fi
  fi
  omarchy plugin disable "$plugin_id"
fi
for name in ui-notes ui-notes-panel; do
  link=$install_dir/$name
  if [[ -L $link && $(readlink -- "$link") == "$plugin_dir/bin/$name" ]]; then
    rm -- "$link"
  fi
done
if [[ -f $plugin_dir/.ui-notes-install && $(cat "$plugin_dir/.ui-notes-install") == "$plugin_id" \
  && -f $plugin_dir/.ui-notes-files ]]; then
  omarchy plugin validate "$plugin_dir"
  cd "$plugin_dir"
  while IFS= read -r record; do
    checksum=${record%% *}
    file=${record#*  }
    # Only the relative regular files recorded by install.sh may be removed.
    [[ $checksum =~ ^[0-9a-f]{64}$ && $file == ./* && $file != *'..'* ]] || continue
    [[ -f $file && ! -L $file ]] || continue
    if [[ $(sha256sum -- "$file") == "$record" ]]; then
      rm -- "$file"
      parent=${file%/*}
      while [[ $parent != . ]]; do
        rmdir -- "$parent" 2>/dev/null || break
        parent=${parent%/*}
      done
    else
      echo "Preserved modified file: $plugin_dir/${file#./}"
    fi
  done < .ui-notes-files
  rm -- .ui-notes-files .ui-notes-install
  cd /
  rmdir -- "$plugin_dir" 2>/dev/null || true
elif [[ -d $plugin_dir ]]; then
  echo "Kept the source checkout. Remove it with: omarchy plugin remove $plugin_id"
fi
if $disable; then
  omarchy-shell shell rescanPlugins >/dev/null
fi
echo "Removed owned launchers and installation files. Project notes and user settings are preserved."
