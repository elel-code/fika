#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: compare-item-image-renderers.sh [--gate-default-promotion] CUSTOM_THEME_LOG DEFAULT_LOG

Compares two FIKA_PERF_ITEM_VIEW logs for Compact/Icons item image rendering:

  CUSTOM_THEME_LOG: run with FIKA_CUSTOM_THEME_ICONS=1, expected to route
                    theme/MIME icons through the custom item-image paint layer.
  DEFAULT_LOG:      default run, expected to route theme/MIME icons through
                    GPUI img() children while thumbnails stay on the custom
                    image layer.

This is a log comparison helper. It cannot judge subjective smoothness, but it
does identify renderer-policy activation and custom image-layer placeholder,
decode, and retained-image churn.

Options:
  --gate-default-promotion
      Exit non-zero unless the custom theme-icon path is clean enough to be
      considered for becoming the default renderer.
EOF
}

gate_default_promotion=false
if [[ "${1:-}" == "--gate-default-promotion" ]]; then
    gate_default_promotion=true
    shift
fi

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

promotion_gate_state="not requested"
promotion_gate_reasons=()
if [[ "$gate_default_promotion" == true ]]; then
    promotion_gate_state="pass"
    if [[ "$custom_renderer_state" != "custom-image-layer" ]]; then
        promotion_gate_reasons+=("custom log did not route theme icons through the custom image layer")
    fi
    if [[ "$default_renderer_state" != "default-gpui-theme-icons" ]]; then
        promotion_gate_reasons+=("default log did not show GPUI theme-icon elements")
    fi
    if (( custom_image_frames == 0 )); then
        promotion_gate_reasons+=("custom log did not include item-image frames")
    fi
    if (( custom_theme_placeholder > 0 )); then
        promotion_gate_reasons+=("custom log still has theme_placeholder churn")
    fi
    if (( custom_theme_decoded > 0 )); then
        promotion_gate_reasons+=("custom log still has theme_decoded first-ready churn")
    fi
    if (( default_theme_placeholder > 0 )); then
        promotion_gate_reasons+=("default log unexpectedly has theme placeholders")
    fi
    if (( ${#promotion_gate_reasons[@]} > 0 )); then
        promotion_gate_state="fail"
    fi
fi

promotion_gate_reason_text="none"
if (( ${#promotion_gate_reasons[@]} > 0 )); then
    promotion_gate_reason_text="$(printf '%s; ' "${promotion_gate_reasons[@]}")"
    promotion_gate_reason_text="${promotion_gate_reason_text%; }"
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
- Default-promotion gate: $promotion_gate_state
- Default-promotion reasons: $promotion_gate_reason_text

Custom-theme analyzer summary:

\`\`\`text
$custom_summary
\`\`\`

Default split-renderer analyzer summary:

\`\`\`text
$default_summary
\`\`\`
EOF

if [[ "$promotion_gate_state" == "fail" ]]; then
    exit 1
fi
