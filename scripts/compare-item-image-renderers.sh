#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: compare-item-image-renderers.sh CUSTOM_THEME_LOG DEFAULT_LOG

Compares two FIKA_PERF_ITEM_VIEW logs for Compact/Icons item image rendering:

  CUSTOM_THEME_LOG: run with FIKA_CUSTOM_THEME_ICONS=1, expected to route
                    theme/MIME icons through the custom item-image paint layer.
  DEFAULT_LOG:      default run, expected to route theme/MIME icons through
                    GPUI img() children while thumbnails stay on the custom
                    image layer.

This is a log comparison helper. It cannot judge subjective smoothness, but it
does identify renderer-policy activation and custom image-layer placeholder,
decode, and retained-image churn.
EOF
}

if [[ $# -ne 2 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    if [[ $# -eq 1 && ( "$1" == "-h" || "$1" == "--help" ) ]]; then
        exit 0
    fi
    exit 2
fi

custom_log="$1"
default_log="$2"
root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
analyzer="$root_dir/scripts/analyze-item-view-perf.sh"

custom_summary="$("$analyzer" "$custom_log")"
default_summary="$("$analyzer" "$default_log")"

metric() {
    local summary="$1"
    local line_name="$2"
    local key="$3"
    printf '%s\n' "$summary" | awk -v line_name="$line_name" -v key="$key" '
        $1 == line_name {
            if (key == substr(line_name, 1, length(line_name) - 1)) {
                print $2 + 0
                found = 1
                exit
            }
            prefix = key "="
            for (i = 2; i <= NF; i++) {
                if (index($i, prefix) == 1) {
                    value = substr($i, length(prefix) + 1)
                    sub(/us$/, "", value)
                    print value
                    found = 1
                    exit
                }
            }
        }
        END {
            if (!found) {
                print 0
            }
        }
    '
}

custom_image_frames="$(metric "$custom_summary" "image_frames:" "image_frames")"
custom_image_layer="$(metric "$custom_summary" "renderer_policy_frames:" "max_image_layer")"
custom_gpui_image="$(metric "$custom_summary" "renderer_policy_frames:" "max_gpui_image_element")"
custom_theme_loaded="$(metric "$custom_summary" "image_sources:" "theme_loaded")"
custom_theme_decoded="$(metric "$custom_summary" "image_sources:" "theme_decoded")"
custom_theme_retained="$(metric "$custom_summary" "image_sources:" "theme_retained")"
custom_theme_placeholder="$(metric "$custom_summary" "image_sources:" "theme_placeholder")"
custom_thumb_fallback="$(metric "$custom_summary" "image_sources:" "thumb_fallback")"
custom_image_paint="$(metric "$custom_summary" "image_frames:" "max_paint")"

default_image_frames="$(metric "$default_summary" "image_frames:" "image_frames")"
default_image_layer="$(metric "$default_summary" "renderer_policy_frames:" "max_image_layer")"
default_gpui_image="$(metric "$default_summary" "renderer_policy_frames:" "max_gpui_image_element")"
default_theme_placeholder="$(metric "$default_summary" "image_sources:" "theme_placeholder")"
default_image_paint="$(metric "$default_summary" "image_frames:" "max_paint")"

custom_renderer_state="unexpected"
if (( custom_image_layer > 0 && custom_gpui_image == 0 )); then
    custom_renderer_state="custom-image-layer"
fi

default_renderer_state="unexpected"
if (( default_gpui_image > 0 )); then
    default_renderer_state="default-gpui-theme-icons"
fi

placeholder_judgement="no custom first-load placeholder evidence"
if (( custom_theme_placeholder > 0 )); then
    placeholder_judgement="custom image layer showed first-load theme placeholders"
fi

decode_judgement="no theme decode completion churn in custom log"
if (( custom_theme_decoded > 0 )); then
    decode_judgement="custom image layer observed GPUI theme decode completion"
fi

cat <<EOF
## Item Image Renderer A/B Evidence

- Custom-theme log: \`$custom_log\`
- Default split-renderer log: \`$default_log\`

| Metric | Custom theme-icons | Default split renderer |
| --- | ---: | ---: |
| renderer max image_layer | $custom_image_layer | $default_image_layer |
| renderer max gpui_image_element | $custom_gpui_image | $default_gpui_image |
| item-image frames | $custom_image_frames | $default_image_frames |
| max item-image paint us | $custom_image_paint | $default_image_paint |
| theme loaded | $custom_theme_loaded | 0 |
| theme decoded first-ready | $custom_theme_decoded | 0 |
| theme retained | $custom_theme_retained | 0 |
| theme placeholder | $custom_theme_placeholder | $default_theme_placeholder |
| thumbnail fallback | $custom_thumb_fallback | 0 |

Automated interpretation:

- Custom-theme renderer state: $custom_renderer_state
- Default renderer state: $default_renderer_state
- Placeholder evidence: $placeholder_judgement
- Decode evidence: $decode_judgement
- Retained same-icon evidence: theme_retained=$custom_theme_retained

Custom-theme analyzer summary:

\`\`\`text
$custom_summary
\`\`\`

Default split-renderer analyzer summary:

\`\`\`text
$default_summary
\`\`\`
EOF
