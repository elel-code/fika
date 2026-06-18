#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: check-item-view-runtime-log.sh LOG

Runs the standard post-P11e item-view runtime perf-log gates against a saved
FIKA_PERF_ITEM_VIEW=1 log. This script does not replace the manual DnD and
rename smoke checklist.
EOF
}

if [[ $# -ne 1 || "$1" == "-h" || "$1" == "--help" ]]; then
    usage
    if [[ $# -eq 1 && ( "$1" == "-h" || "$1" == "--help" ) ]]; then
        exit 0
    fi
    exit 2
fi

log_path="$1"
root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

"$root_dir/scripts/analyze-item-view-perf.sh" \
    --require-steady \
    --require-details \
    --require-static-visual \
    --require-static-modes Compact,Icons \
    --require-interaction \
    --require-renderer-policy \
    --expect-retained-item-policy \
    --require-paint-slots \
    --require-renderer-policy-modes Compact,Icons,Details \
    --require-modes Compact,Icons,Details \
    --steady-total-us 1000 \
    --file-grid-build-us 3000 \
    --static-visual-paint-us 3000 \
    --image-paint-us 3000 \
    --custom-paint-us 3000 \
    "$log_path"
