#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: summarize-item-view-renderer-evidence.sh LOG

Runs the standard item-view runtime perf-log gate, then prints a Markdown
evidence block that can be copied into docs/ITEM_VIEW_RENDERER_DECISIONS.md.
This summarizes perf-log evidence only; manual DnD and rename smoke results must
still be recorded by a human reviewer.
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
runtime_gate="$root_dir/scripts/check-item-view-runtime-log.sh"

summary="$("$runtime_gate" "$log_path")"

cat <<EOF
## Item View Renderer Evidence

- Log: \`$log_path\`
- Perf gate: \`scripts/check-item-view-runtime-log.sh $log_path\`
- Manual review still required: DnD and rename checklist from \`docs/ITEM_VIEW_RUNTIME_SMOKE.md\`

\`\`\`text
$summary
\`\`\`

Renderer decision follow-up:

- Compact/Icons base visuals: keep or revisit custom paint using static visual and custom paint maxima above.
- Compact/Icons image layer: keep or revisit custom paint using item image and custom paint maxima above.
- Details visual layer: keep or revisit custom paint using details visual and shape-cache channels above.
- Renderer surface counts: compare renderer_policy_frames against docs/ITEM_VIEW_RENDERER_DECISIONS.md.
- Drag-start shells: do not remove based on perf logs alone; the GPUI drag-start API gate still applies.
EOF
