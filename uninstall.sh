#!/bin/bash
set -euo pipefail

plugin_id=cordrogue.omatate
plugin_dir=${OMATATE_PLUGIN_DIR:-${UI_NOTES_PLUGIN_DIR:-$HOME/.config/omarchy/plugins/$plugin_id}}
install_dir=${OMATATE_INSTALL_DIR:-${UI_NOTES_INSTALL_DIR:-$HOME/.local/bin}}
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
    if flush_reply=$("$install_dir/omatate" panel quit 2>&1); then
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
for name in omatate omatate-panel ui-notes ui-notes-panel; do
  link=$install_dir/$name
  if [[ -L $link && $(readlink -- "$link") == "$plugin_dir/bin/$name" ]]; then
    rm -- "$link"
  fi
done
if [[ -f $plugin_dir/.omatate-install && $(cat "$plugin_dir/.omatate-install") == "$plugin_id" \
  && -f $plugin_dir/.omatate-files ]]; then
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
  done < .omatate-files
  rm -- .omatate-files .omatate-install
  cd /
  rmdir -- "$plugin_dir" 2>/dev/null || true
elif [[ -d $plugin_dir ]]; then
  echo "Kept the source checkout. Remove it with: omarchy plugin remove $plugin_id"
fi
if $disable; then
  omarchy-shell shell rescanPlugins >/dev/null
fi
echo "Removed owned launchers and installation files. Project notes and user settings are preserved."
