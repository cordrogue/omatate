#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
default_input="$script_dir/qml"

if runner=$(command -v qmltestrunner 2>/dev/null); then
    :
elif [[ -x /usr/lib/qt6/bin/qmltestrunner ]]; then
    runner=/usr/lib/qt6/bin/qmltestrunner
else
    printf '%s\n' \
        'error: qmltestrunner was not found in PATH or at /usr/lib/qt6/bin/qmltestrunner.' \
        'Install the Qt 6 Quick Test tools, then run this script again.' >&2
    exit 127
fi

runner_args=("$@")
use_default_input=true
for arg in "$@"; do
    case "$arg" in
        -help|-h|--help|-input|-input=*)
            use_default_input=false
            ;;
    esac
done
if $use_default_input; then
    runner_args=(-input "$default_input" "${runner_args[@]}")
fi

exec env \
    -u QSG_RHI_BACKEND \
    QT_QPA_PLATFORM=offscreen \
    QT_QPA_PLATFORMTHEME= \
    QT_STYLE_OVERRIDE=Fusion \
    QT_QUICK_BACKEND=software \
    QT_QUICK_CONTROLS_STYLE=Basic \
    "$runner" "${runner_args[@]}"
