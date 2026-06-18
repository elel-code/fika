#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: analyze-item-view-perf.sh [OPTIONS] LOG
       FIKA_PERF_ITEM_VIEW=1 target/debug/fika ~/Downloads 2>&1 | analyze-item-view-perf.sh [OPTIONS] -

Summarizes FIKA_PERF_ITEM_VIEW item-view logs and optionally enforces perf-log
acceptance gates.

Options:
  --require-steady
      Fail if no [fika item-view] phase=steady frame is present.

  --require-details
      Fail if Details-specific visual and shape-cache channels are missing.

  --require-static-visual
      Fail if [fika static-item-visual] Compact/Icons paint timing is missing.

  --require-static-modes A,B,C
      Fail if any comma-separated view mode is absent from static visual logs.

  --require-interaction
      Fail if [fika item-interaction] hitbox timing is missing.

  --require-renderer-policy
      Fail if [fika renderer-policy] surface-count logs are missing.

  --require-paint-slots
      Fail if [fika item-paint-slots] retained slot activity logs are missing.

  --require-autosmoke
      Fail if item-view autosmoke start/complete and scenario actions are missing.

  --require-renderer-policy-modes A,B,C
      Fail if any comma-separated view mode is absent from renderer-policy logs.

  --require-modes A,B,C
      Fail if any comma-separated view mode is absent from parsed perf logs.

  --steady-total-us N
      Fail if any item-view phase=steady total exceeds N microseconds.

  --file-grid-build-us N
      Fail if any [fika file-grid] build exceeds N microseconds.

  --static-visual-paint-us N
      Fail if any [fika static-item-visual] paint exceeds N microseconds.

  --image-paint-us N
      Fail if any [fika item-image] paint exceeds N microseconds.

  --custom-paint-us N
      Fail if any custom paint channel exceeds N microseconds.

  -h, --help
      Show this help.
EOF
}

require_steady=false
require_details=false
require_static_visual=false
require_interaction=false
require_renderer_policy=false
require_paint_slots=false
require_autosmoke=false
required_modes=""
required_static_modes=""
required_renderer_policy_modes=""
steady_total_us=""
file_grid_build_us=""
static_visual_paint_us=""
image_paint_us=""
custom_paint_us=""
log_path=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --require-steady)
            require_steady=true
            ;;
        --require-details)
            require_details=true
            ;;
        --require-static-visual)
            require_static_visual=true
            ;;
        --require-static-modes)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--require-static-modes requires a comma-separated value" >&2
                usage >&2
                exit 2
            fi
            required_static_modes="$2"
            shift
            ;;
        --require-static-modes=*)
            required_static_modes="${1#--require-static-modes=}"
            ;;
        --require-interaction)
            require_interaction=true
            ;;
        --require-renderer-policy)
            require_renderer_policy=true
            ;;
        --require-paint-slots)
            require_paint_slots=true
            ;;
        --require-autosmoke)
            require_autosmoke=true
            ;;
        --require-renderer-policy-modes)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--require-renderer-policy-modes requires a comma-separated value" >&2
                usage >&2
                exit 2
            fi
            required_renderer_policy_modes="$2"
            shift
            ;;
        --require-renderer-policy-modes=*)
            required_renderer_policy_modes="${1#--require-renderer-policy-modes=}"
            ;;
        --require-modes)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--require-modes requires a comma-separated value" >&2
                usage >&2
                exit 2
            fi
            required_modes="$2"
            shift
            ;;
        --require-modes=*)
            required_modes="${1#--require-modes=}"
            ;;
        --steady-total-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--steady-total-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            steady_total_us="$2"
            shift
            ;;
        --steady-total-us=*)
            steady_total_us="${1#--steady-total-us=}"
            ;;
        --file-grid-build-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--file-grid-build-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            file_grid_build_us="$2"
            shift
            ;;
        --file-grid-build-us=*)
            file_grid_build_us="${1#--file-grid-build-us=}"
            ;;
        --static-visual-paint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--static-visual-paint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            static_visual_paint_us="$2"
            shift
            ;;
        --static-visual-paint-us=*)
            static_visual_paint_us="${1#--static-visual-paint-us=}"
            ;;
        --image-paint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--image-paint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            image_paint_us="$2"
            shift
            ;;
        --image-paint-us=*)
            image_paint_us="${1#--image-paint-us=}"
            ;;
        --custom-paint-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--custom-paint-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            custom_paint_us="$2"
            shift
            ;;
        --custom-paint-us=*)
            custom_paint_us="${1#--custom-paint-us=}"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --*)
            echo "unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
        *)
            if [[ -n "$log_path" ]]; then
                echo "only one LOG path is supported" >&2
                usage >&2
                exit 2
            fi
            log_path="$1"
            ;;
    esac
    shift
done

if [[ -z "$log_path" ]]; then
    echo "LOG path is required; use - for stdin" >&2
    usage >&2
    exit 2
fi

if [[ -n "$steady_total_us" && ! "$steady_total_us" =~ ^[0-9]+$ ]]; then
    echo "--steady-total-us must be an integer microsecond value" >&2
    exit 2
fi

if [[ -n "$file_grid_build_us" && ! "$file_grid_build_us" =~ ^[0-9]+$ ]]; then
    echo "--file-grid-build-us must be an integer microsecond value" >&2
    exit 2
fi

if [[ -n "$static_visual_paint_us" && ! "$static_visual_paint_us" =~ ^[0-9]+$ ]]; then
    echo "--static-visual-paint-us must be an integer microsecond value" >&2
    exit 2
fi

if [[ -n "$image_paint_us" && ! "$image_paint_us" =~ ^[0-9]+$ ]]; then
    echo "--image-paint-us must be an integer microsecond value" >&2
    exit 2
fi

if [[ -n "$custom_paint_us" && ! "$custom_paint_us" =~ ^[0-9]+$ ]]; then
    echo "--custom-paint-us must be an integer microsecond value" >&2
    exit 2
fi

awk \
    -v require_steady="$require_steady" \
    -v require_details="$require_details" \
    -v require_static_visual="$require_static_visual" \
    -v require_interaction="$require_interaction" \
    -v require_renderer_policy="$require_renderer_policy" \
    -v require_paint_slots="$require_paint_slots" \
    -v require_autosmoke="$require_autosmoke" \
    -v required_modes="$required_modes" \
    -v required_static_modes="$required_static_modes" \
    -v required_renderer_policy_modes="$required_renderer_policy_modes" \
    -v steady_total_limit="$steady_total_us" \
    -v file_grid_build_limit="$file_grid_build_us" \
    -v static_visual_paint_limit="$static_visual_paint_us" \
    -v image_paint_limit="$image_paint_us" \
    -v custom_paint_limit="$custom_paint_us" '
function trim(value) {
    sub(/^[[:space:]]+/, "", value)
    sub(/[[:space:]]+$/, "", value)
    return value
}

function field(name,    prefix, i, value) {
    prefix = name "="
    for (i = 1; i <= NF; i++) {
        if (index($i, prefix) == 1) {
            value = substr($i, length(prefix) + 1)
            sub(/,$/, "", value)
            return value
        }
    }
    return ""
}

function us_field(name,    value) {
    value = field(name)
    sub(/us$/, "", value)
    return value + 0
}

function note_mode(mode) {
    if (mode != "") {
        modes[mode] = 1
    }
}

function max_assign(array, key, value) {
    if (!(key in array) || value > array[key]) {
        array[key] = value
    }
}

function fail(message) {
    print "fail: " message > "/dev/stderr"
    failures++
}

function record_custom_paint(prepaint, paint) {
    custom_paint_count++
    max_assign(single_max, "custom_paint_prepaint", prepaint)
    max_assign(single_max, "custom_paint_paint", paint)
    if (custom_paint_limit != "" && paint > custom_paint_limit + 0) {
        custom_paint_over_limit++
    }
}

function check_renderer_policy_count(name, items, value, mode) {
    if (value < 0) {
        fail("renderer-policy " name " is negative in mode " mode)
    }
    if (value > items) {
        fail("renderer-policy " name " exceeds items in mode " mode)
    }
}

function parse_required_list(list, target, label,    count, i, value) {
    if (list == "") {
        return
    }
    count = split(list, values, ",")
    for (i = 1; i <= count; i++) {
        value = trim(values[i])
        if (value == "") {
            fail("empty mode in " label)
        } else {
            target[value] = 1
        }
    }
}

BEGIN {
    parse_required_list(required_modes, required_mode, "--require-modes")
    parse_required_list(required_static_modes, required_static_mode, "--require-static-modes")
    parse_required_list(required_renderer_policy_modes, required_renderer_policy_mode, "--require-renderer-policy-modes")
}

/^\[fika item-view\]/ {
    item_view_count++
    mode = field("mode")
    phase = field("phase")
    if (phase == "") {
        phase = "unknown"
    }
    total = us_field("total")
    raw = us_field("raw")
    icon_sync = us_field("icon_sync")
    queue = us_field("queue")
    convert = us_field("convert")
    visible = field("visible") + 0
    note_mode(mode)
    phase_count[phase]++
    max_assign(phase_max_total, phase, total)
    max_assign(phase_max_visible, phase, visible)
    max_assign(single_max, "item_view_raw", raw)
    max_assign(single_max, "item_view_icon_sync", icon_sync)
    max_assign(single_max, "item_view_queue", queue)
    max_assign(single_max, "item_view_convert", convert)
    if (phase == "steady") {
        steady_count++
        if (steady_total_limit != "" && total > steady_total_limit + 0) {
            steady_over_limit++
        }
    }
}

/^\[fika file-grid\]/ {
    file_grid_count++
    note_mode(field("mode"))
    build = us_field("build")
    max_assign(single_max, "file_grid_build", build)
    if (file_grid_build_limit != "" && build > file_grid_build_limit + 0) {
        file_grid_over_limit++
    }
}

/^\[fika static-item-visual\]/ {
    static_visual_count++
    mode = field("mode")
    note_mode(mode)
    if (mode != "") {
        static_visual_modes[mode] = 1
    }
    paint = us_field("paint")
    prepaint = us_field("prepaint")
    max_assign(single_max, "static_visual_prepaint", prepaint)
    max_assign(single_max, "static_visual_paint", paint)
    record_custom_paint(prepaint, paint)
    if (static_visual_paint_limit != "" && paint > static_visual_paint_limit + 0) {
        static_visual_over_limit++
    }
}

/^\[fika item-image\]/ {
    image_count++
    note_mode(field("mode"))
    paint = us_field("paint")
    prepaint = us_field("prepaint")
    image_theme_loaded += field("theme_loaded") + 0
    image_theme_decoded += field("theme_decoded") + 0
    image_theme_retained += field("theme_retained") + 0
    image_theme_placeholder += field("theme_placeholder") + 0
    image_thumb_loaded += field("thumb_loaded") + 0
    image_thumb_decoded += field("thumb_decoded") + 0
    image_thumb_retained += field("thumb_retained") + 0
    image_thumb_fallback += field("thumb_fallback") + 0
    max_assign(single_max, "image_prepaint", prepaint)
    max_assign(single_max, "image_paint", paint)
    record_custom_paint(prepaint, paint)
    if (image_paint_limit != "" && paint > image_paint_limit + 0) {
        image_over_limit++
    }
}

/^\[fika details-visual\]/ {
    details_visual_count++
    note_mode(field("mode"))
    prepaint = us_field("prepaint")
    paint = us_field("paint")
    max_assign(single_max, "details_visual_prepaint", prepaint)
    max_assign(single_max, "details_visual_paint", paint)
    record_custom_paint(prepaint, paint)
}

/^\[fika details-shape-cache\]/ {
    details_shape_count++
    note_mode(field("mode"))
    details_shape_hits += field("hits") + 0
    details_shape_misses += field("misses") + 0
}

/^\[fika item-interaction\]/ {
    item_interaction_count++
    note_mode(field("mode"))
    max_assign(single_max, "interaction_prepaint", us_field("prepaint"))
    max_assign(single_max, "interaction_paint", us_field("paint"))
    max_assign(single_max, "interaction_prepaint_count", field("prepaint_count") + 0)
    max_assign(single_max, "interaction_paint_count", field("paint_count") + 0)
}

/^\[fika item-paint-slots\]/ {
    item_paint_slot_count++
    note_mode(field("mode"))
    max_assign(single_max, "paint_slot_inserted", field("inserted") + 0)
    max_assign(single_max, "paint_slot_content", field("content") + 0)
    max_assign(single_max, "paint_slot_geometry", field("geometry") + 0)
    max_assign(single_max, "paint_slot_visual", field("visual") + 0)
    max_assign(single_max, "paint_slot_unchanged", field("unchanged") + 0)
    max_assign(single_max, "paint_slot_removed", field("removed") + 0)
    max_assign(single_max, "paint_slot_entries", field("entries") + 0)
}

/^\[fika renderer-policy\]/ {
    renderer_policy_count++
    mode = field("mode")
    items = field("items") + 0
    visual_layer = field("visual_layer") + 0
    image_layer = field("image_layer") + 0
    gpui_image_element = field("gpui_image_element") + 0
    retained_interaction = field("retained_interaction") + 0
    gpui_drag_shell = field("gpui_drag_shell") + 0
    rename_overlay = field("rename_overlay") + 0
    note_mode(mode)
    if (mode != "") {
        renderer_policy_modes[mode] = 1
    }
    if (items < 0) {
        fail("renderer-policy items is negative in mode " mode)
    }
    check_renderer_policy_count("visual_layer", items, visual_layer, mode)
    check_renderer_policy_count("image_layer", items, image_layer, mode)
    check_renderer_policy_count("gpui_image_element", items, gpui_image_element, mode)
    check_renderer_policy_count("retained_interaction", items, retained_interaction, mode)
    check_renderer_policy_count("gpui_drag_shell", items, gpui_drag_shell, mode)
    check_renderer_policy_count("rename_overlay", items, rename_overlay, mode)
    max_assign(single_max, "renderer_policy_items", items)
    max_assign(single_max, "renderer_policy_visual_layer", visual_layer)
    max_assign(single_max, "renderer_policy_image_layer", image_layer)
    max_assign(single_max, "renderer_policy_gpui_image_element", gpui_image_element)
    max_assign(single_max, "renderer_policy_retained_interaction", retained_interaction)
    max_assign(single_max, "renderer_policy_gpui_drag_shell", gpui_drag_shell)
    max_assign(single_max, "renderer_policy_rename_overlay", rename_overlay)
}

/^\[fika autosmoke\] item-view start / {
    autosmoke_start_seen++
    scenario = field("scenario")
    if (scenario != "") {
        if (autosmoke_start_scenario != "" && autosmoke_start_scenario != scenario) {
            autosmoke_scenario_conflict = 1
        }
        autosmoke_start_scenario = scenario
    }
}

/^\[fika autosmoke\] item-view complete / {
    autosmoke_complete_seen++
    scenario = field("scenario")
    if (scenario != "") {
        if (autosmoke_complete_scenario != "" && autosmoke_complete_scenario != scenario) {
            autosmoke_scenario_conflict = 1
        }
        autosmoke_complete_scenario = scenario
    }
}

/^\[fika autosmoke\] item-view action=/ {
    action = field("action")
    changed = field("changed")
    if (action == "zoom-in") {
        autosmoke_zoom_in_seen++
    } else if (action == "zoom-out") {
        autosmoke_zoom_out_seen++
    } else if (action == "scroll-forward" && changed == "true") {
        autosmoke_scroll_forward_seen++
    } else if (action == "scroll-back" && changed == "true") {
        autosmoke_scroll_back_seen++
    }
}

END {
    print "Item view perf summary"
    print "  item_view_frames: " item_view_count
    for (i = 1; i <= split("initial mode-switch content-change geometry-change visual-change steady unknown", ordered, " "); i++) {
        phase = ordered[i]
        if (phase in phase_count) {
            printf "  phase %-15s frames=%d max_total=%dus max_visible=%d\n",
                phase, phase_count[phase], phase_max_total[phase], phase_max_visible[phase]
        }
    }

    print "  item_view_stage_max:" \
        " raw=" (("item_view_raw" in single_max) ? single_max["item_view_raw"] : 0) "us" \
        " icon_sync=" (("item_view_icon_sync" in single_max) ? single_max["item_view_icon_sync"] : 0) "us" \
        " queue=" (("item_view_queue" in single_max) ? single_max["item_view_queue"] : 0) "us" \
        " convert=" (("item_view_convert" in single_max) ? single_max["item_view_convert"] : 0) "us"
    modes_text = ""
    for (mode in modes) {
        modes_text = modes_text (modes_text == "" ? "" : ",") mode
    }
    print "  modes: " (modes_text == "" ? "<none>" : modes_text)
    print "  file_grid_frames: " (file_grid_count + 0) " max_build=" (("file_grid_build" in single_max) ? single_max["file_grid_build"] : 0) "us"
    print "  static_visual_frames: " (static_visual_count + 0) \
        " max_prepaint=" (("static_visual_prepaint" in single_max) ? single_max["static_visual_prepaint"] : 0) "us" \
        " max_paint=" (("static_visual_paint" in single_max) ? single_max["static_visual_paint"] : 0) "us"
    print "  image_frames: " (image_count + 0) \
        " max_prepaint=" (("image_prepaint" in single_max) ? single_max["image_prepaint"] : 0) "us" \
        " max_paint=" (("image_paint" in single_max) ? single_max["image_paint"] : 0) "us"
    print "  image_sources: theme_loaded=" (image_theme_loaded + 0) \
        " theme_decoded=" (image_theme_decoded + 0) \
        " theme_retained=" (image_theme_retained + 0) \
        " theme_placeholder=" (image_theme_placeholder + 0) \
        " thumb_loaded=" (image_thumb_loaded + 0) \
        " thumb_decoded=" (image_thumb_decoded + 0) \
        " thumb_retained=" (image_thumb_retained + 0) \
        " thumb_fallback=" (image_thumb_fallback + 0)
    print "  custom_paint_frames: " (custom_paint_count + 0) \
        " max_prepaint=" (("custom_paint_prepaint" in single_max) ? single_max["custom_paint_prepaint"] : 0) "us" \
        " max_paint=" (("custom_paint_paint" in single_max) ? single_max["custom_paint_paint"] : 0) "us"
    print "  details_visual_frames: " (details_visual_count + 0) \
        " max_prepaint=" (("details_visual_prepaint" in single_max) ? single_max["details_visual_prepaint"] : 0) "us" \
        " max_paint=" (("details_visual_paint" in single_max) ? single_max["details_visual_paint"] : 0) "us"
    print "  details_shape_frames: " (details_shape_count + 0) \
        " hits=" (details_shape_hits + 0) " misses=" (details_shape_misses + 0)
    print "  interaction_frames: " (item_interaction_count + 0) \
        " max_prepaint_count=" (("interaction_prepaint_count" in single_max) ? single_max["interaction_prepaint_count"] : 0) \
        " max_paint_count=" (("interaction_paint_count" in single_max) ? single_max["interaction_paint_count"] : 0)
    print "  item_paint_slots_frames: " (item_paint_slot_count + 0) \
        " max_inserted=" (("paint_slot_inserted" in single_max) ? single_max["paint_slot_inserted"] : 0) \
        " max_content=" (("paint_slot_content" in single_max) ? single_max["paint_slot_content"] : 0) \
        " max_geometry=" (("paint_slot_geometry" in single_max) ? single_max["paint_slot_geometry"] : 0) \
        " max_visual=" (("paint_slot_visual" in single_max) ? single_max["paint_slot_visual"] : 0) \
        " max_unchanged=" (("paint_slot_unchanged" in single_max) ? single_max["paint_slot_unchanged"] : 0) \
        " max_removed=" (("paint_slot_removed" in single_max) ? single_max["paint_slot_removed"] : 0) \
        " max_entries=" (("paint_slot_entries" in single_max) ? single_max["paint_slot_entries"] : 0)
    print "  renderer_policy_frames: " (renderer_policy_count + 0) \
        " max_items=" (("renderer_policy_items" in single_max) ? single_max["renderer_policy_items"] : 0) \
        " max_visual_layer=" (("renderer_policy_visual_layer" in single_max) ? single_max["renderer_policy_visual_layer"] : 0) \
        " max_image_layer=" (("renderer_policy_image_layer" in single_max) ? single_max["renderer_policy_image_layer"] : 0) \
        " max_gpui_image_element=" (("renderer_policy_gpui_image_element" in single_max) ? single_max["renderer_policy_gpui_image_element"] : 0) \
        " max_retained_interaction=" (("renderer_policy_retained_interaction" in single_max) ? single_max["renderer_policy_retained_interaction"] : 0) \
        " max_gpui_drag_shell=" (("renderer_policy_gpui_drag_shell" in single_max) ? single_max["renderer_policy_gpui_drag_shell"] : 0) \
        " max_rename_overlay=" (("renderer_policy_rename_overlay" in single_max) ? single_max["renderer_policy_rename_overlay"] : 0)
    autosmoke_scenario = autosmoke_start_scenario != "" ? autosmoke_start_scenario : autosmoke_complete_scenario
    print "  autosmoke:" \
        " start=" (autosmoke_start_seen + 0) \
        " complete=" (autosmoke_complete_seen + 0) \
        " scenario=" (autosmoke_scenario == "" ? "<none>" : autosmoke_scenario) \
        " zoom_in=" (autosmoke_zoom_in_seen + 0) \
        " zoom_out=" (autosmoke_zoom_out_seen + 0) \
        " scroll_forward=" (autosmoke_scroll_forward_seen + 0) \
        " scroll_back=" (autosmoke_scroll_back_seen + 0)

    if (item_view_count == 0) {
        fail("missing [fika item-view] lines")
    }
    if (require_steady == "true" && steady_count == 0) {
        fail("missing [fika item-view] phase=steady frames")
    }
    if (require_details == "true" && details_visual_count == 0) {
        fail("missing [fika details-visual] lines")
    }
    if (require_details == "true" && details_shape_count == 0) {
        fail("missing [fika details-shape-cache] lines")
    }
    if (require_static_visual == "true" && static_visual_count == 0) {
        fail("missing [fika static-item-visual] lines")
    }
    for (mode in required_static_mode) {
        if (!(mode in static_visual_modes)) {
            fail("missing required static visual mode " mode)
        }
    }
    if (require_interaction == "true" && item_interaction_count == 0) {
        fail("missing [fika item-interaction] lines")
    }
    if (require_renderer_policy == "true" && renderer_policy_count == 0) {
        fail("missing [fika renderer-policy] lines")
    }
    if (require_paint_slots == "true") {
        if (item_paint_slot_count == 0) {
            fail("missing [fika item-paint-slots] lines")
        }
        if (!(("paint_slot_entries" in single_max) && single_max["paint_slot_entries"] > 0)) {
            fail("missing non-empty retained item paint-slot evidence")
        }
    }
    if (require_autosmoke == "true") {
        if (autosmoke_start_seen == 0 || autosmoke_complete_seen == 0) {
            fail("missing item-view autosmoke start/complete markers")
        }
        if (autosmoke_start_scenario == "" || autosmoke_complete_scenario == "") {
            fail("missing item-view autosmoke scenario markers")
        }
        if (autosmoke_start_scenario != "" && autosmoke_complete_scenario != "" &&
            autosmoke_start_scenario != autosmoke_complete_scenario) {
            fail("item-view autosmoke start/complete scenarios differ")
        }
        if (autosmoke_scenario_conflict) {
            fail("conflicting item-view autosmoke scenarios")
        }
        if (autosmoke_scenario != "Zoom" &&
            autosmoke_scenario != "Scroll" &&
            autosmoke_scenario != "ZoomScroll") {
            fail("unknown item-view autosmoke scenario " autosmoke_scenario)
        }
        if (autosmoke_scenario == "Zoom" || autosmoke_scenario == "ZoomScroll") {
            if (autosmoke_zoom_in_seen == 0 || autosmoke_zoom_out_seen == 0) {
                fail("missing item-view autosmoke zoom action markers")
            }
        }
        if (autosmoke_scenario == "Scroll" || autosmoke_scenario == "ZoomScroll") {
            if (autosmoke_scroll_forward_seen == 0 || autosmoke_scroll_back_seen == 0) {
                fail("missing changed=true item-view autosmoke scroll action markers")
            }
        }
    }
    for (mode in required_renderer_policy_mode) {
        if (!(mode in renderer_policy_modes)) {
            fail("missing required renderer-policy mode " mode)
        }
    }
    for (mode in required_mode) {
        if (!(mode in modes)) {
            fail("missing required mode " mode)
        }
    }
    if (steady_over_limit > 0) {
        fail(steady_over_limit " steady item-view frame(s) exceeded " steady_total_limit "us")
    }
    if (file_grid_over_limit > 0) {
        fail(file_grid_over_limit " file-grid build frame(s) exceeded " file_grid_build_limit "us")
    }
    if (static_visual_over_limit > 0) {
        fail(static_visual_over_limit " static visual paint frame(s) exceeded " static_visual_paint_limit "us")
    }
    if (image_over_limit > 0) {
        fail(image_over_limit " item image paint frame(s) exceeded " image_paint_limit "us")
    }
    if (custom_paint_over_limit > 0) {
        fail(custom_paint_over_limit " custom paint frame(s) exceeded " custom_paint_limit "us")
    }

    exit failures > 0 ? 1 : 0
}
' "$log_path"
