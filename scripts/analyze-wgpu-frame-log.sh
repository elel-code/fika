#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: analyze-wgpu-frame-log.sh [OPTIONS] LOG
       FIKA_LOG=1 target/debug/fika /etc 2>&1 | analyze-wgpu-frame-log.sh -

Summarizes [fika-wgpu] frame logs and optionally enforces render latency gates.

Options:
  --require-frames
      Fail if no wgpu frame log lines are present.

  --max-render-us N
      Fail if any frame render time exceeds N microseconds.

  --warm-max-render-us N
      Fail if any warm frame render time exceeds N microseconds. The first
      frame per view is treated as cold.

  --warm-p95-render-us N
      Fail if warm render p95 exceeds N microseconds.

  --max-text-raster-us N
      Fail if any frame text raster time exceeds N microseconds.

  --max-icon-raster-us N
      Fail if any frame icon raster time exceeds N microseconds.

  -h, --help
      Show this help.
EOF
}

require_frames=false
max_render_us=""
warm_max_render_us=""
warm_p95_render_us=""
max_text_raster_us=""
max_icon_raster_us=""
log_path=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --require-frames)
            require_frames=true
            ;;
        --max-render-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--max-render-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            max_render_us="$2"
            shift
            ;;
        --max-render-us=*)
            max_render_us="${1#--max-render-us=}"
            ;;
        --warm-max-render-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--warm-max-render-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            warm_max_render_us="$2"
            shift
            ;;
        --warm-max-render-us=*)
            warm_max_render_us="${1#--warm-max-render-us=}"
            ;;
        --warm-p95-render-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--warm-p95-render-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            warm_p95_render_us="$2"
            shift
            ;;
        --warm-p95-render-us=*)
            warm_p95_render_us="${1#--warm-p95-render-us=}"
            ;;
        --max-text-raster-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--max-text-raster-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            max_text_raster_us="$2"
            shift
            ;;
        --max-text-raster-us=*)
            max_text_raster_us="${1#--max-text-raster-us=}"
            ;;
        --max-icon-raster-us)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--max-icon-raster-us requires a numeric value" >&2
                usage >&2
                exit 2
            fi
            max_icon_raster_us="$2"
            shift
            ;;
        --max-icon-raster-us=*)
            max_icon_raster_us="${1#--max-icon-raster-us=}"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --*)
            echo "unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
        *)
            if [[ -n "$log_path" ]]; then
                echo "only one LOG path may be provided" >&2
                usage >&2
                exit 2
            fi
            log_path="$1"
            ;;
    esac
    shift
done

if [[ -z "$log_path" ]]; then
    echo "missing LOG path" >&2
    usage >&2
    exit 2
fi

for value in "$max_render_us" "$warm_max_render_us" "$warm_p95_render_us" "$max_text_raster_us" "$max_icon_raster_us"; do
    if [[ -n "$value" && ! "$value" =~ ^[0-9]+$ ]]; then
        echo "gate values must be integer microsecond values" >&2
        exit 2
    fi
done

input_path="$log_path"
if [[ "$log_path" == "-" ]]; then
    input_path="/dev/stdin"
elif [[ ! -s "$log_path" ]]; then
    echo "missing or empty log: $log_path" >&2
    exit 1
fi

awk \
    -v require_frames="$require_frames" \
    -v max_render_us="${max_render_us:-}" \
    -v warm_max_render_us="${warm_max_render_us:-}" \
    -v warm_p95_render_us="${warm_p95_render_us:-}" \
    -v max_text_raster_us="${max_text_raster_us:-}" \
    -v max_icon_raster_us="${max_icon_raster_us:-}" '
function value_of(key,    i, pair) {
    for (i = 1; i <= NF; i++) {
        split($i, pair, "=")
        if (pair[1] == key) {
            return pair[2]
        }
    }
    return ""
}

function numeric_value(key,    value) {
    value = value_of(key)
    gsub(/[^0-9.].*/, "", value)
    return value + 0
}

function bump_metric(prefix, metric, value,    key) {
    key = prefix SUBSEP metric
    if (!(key in max) || value > max[key]) {
        max[key] = value
    }
}

function add_render(prefix, value, warm,    key) {
    key = prefix SUBSEP "render"
    render_count[key]++
    render_values[key, render_count[key]] = value
    if (warm) {
        warm_key = prefix SUBSEP "warm_render"
        render_count[warm_key]++
        render_values[warm_key, render_count[warm_key]] = value
    }
}

function percentile(prefix, metric, percent,    key, count, i, j, tmp, rank) {
    key = prefix SUBSEP metric
    count = render_count[key] + 0
    if (count <= 0) {
        return 0
    }
    delete sorted
    for (i = 1; i <= count; i++) {
        sorted[i] = render_values[key, i]
    }
    for (i = 1; i <= count; i++) {
        for (j = i + 1; j <= count; j++) {
            if (sorted[j] < sorted[i]) {
                tmp = sorted[i]
                sorted[i] = sorted[j]
                sorted[j] = tmp
            }
        }
    }
    rank = int((percent * count + 99) / 100)
    if (rank < 1) {
        rank = 1
    }
    if (rank > count) {
        rank = count
    }
    return sorted[rank]
}

function add_prewarm(prefix, resolve, entries, deferred, over_budget,    key) {
    prewarm_count[prefix]++
    prewarm_values[prefix, prewarm_count[prefix]] = resolve
    bump_metric(prefix, "prewarm_resolve", resolve)
    bump_metric(prefix, "prewarm_entries", entries)
    bump_metric(prefix, "prewarm_deferred", deferred)
    bump_metric(prefix, "prewarm_over_budget", over_budget)
}

function prewarm_percentile(prefix, percent,    count, i, j, tmp, rank) {
    count = prewarm_count[prefix] + 0
    if (count <= 0) {
        return 0
    }
    delete sorted
    for (i = 1; i <= count; i++) {
        sorted[i] = prewarm_values[prefix, i]
    }
    for (i = 1; i <= count; i++) {
        for (j = i + 1; j <= count; j++) {
            if (sorted[j] < sorted[i]) {
                tmp = sorted[i]
                sorted[i] = sorted[j]
                sorted[j] = tmp
            }
        }
    }
    rank = int((percent * count + 99) / 100)
    if (rank < 1) {
        rank = 1
    }
    if (rank > count) {
        rank = count
    }
    return sorted[rank]
}

function print_summary(prefix, label,    frames, warm_frames, render_max, render_p50, render_p95, warm_p95, warm_max_value) {
    frames = frame_count[prefix] + 0
    if (frames <= 0) {
        return
    }
    warm_frames = render_count[prefix SUBSEP "warm_render"] + 0
    render_max = max[prefix SUBSEP "render"] + 0
    render_p50 = percentile(prefix, "render", 50)
    render_p95 = percentile(prefix, "render", 95)
    warm_p95 = percentile(prefix, "warm_render", 95)
    warm_max_value = max[prefix SUBSEP "warm_render"] + 0
    printf("wgpu-frame-summary scope=%s frames=%d warm_frames=%d render_us_p50=%d render_us_p95=%d render_us_max=%d warm_render_us_p95=%d warm_render_us_max=%d layout_us_max=%d text_raster_us_max=%d icon_resolve_us_max=%d icon_raster_us_max=%d text_deferred_max=%d icon_deferred_max=%d icon_raster_deferred_max=%d visible_max=%d\n",
        label,
        frames,
        warm_frames,
        render_p50,
        render_p95,
        render_max,
        warm_p95,
        warm_max_value,
        max[prefix SUBSEP "layout"] + 0,
        max[prefix SUBSEP "text_raster"] + 0,
        max[prefix SUBSEP "icon_resolve"] + 0,
        max[prefix SUBSEP "icon_raster"] + 0,
        max[prefix SUBSEP "text_deferred"] + 0,
        max[prefix SUBSEP "icon_deferred"] + 0,
        max[prefix SUBSEP "icon_raster_deferred"] + 0,
        max[prefix SUBSEP "visible"] + 0)
}

function print_prewarm_summary(prefix, label,    count, resolve_p50, resolve_p95) {
    count = prewarm_count[prefix] + 0
    if (count <= 0) {
        return
    }
    resolve_p50 = prewarm_percentile(prefix, 50)
    resolve_p95 = prewarm_percentile(prefix, 95)
    printf("wgpu-prewarm-summary scope=%s samples=%d resolve_us_p50=%d resolve_us_p95=%d resolve_us_max=%d entries_max=%d deferred_max=%d over_budget_max=%d\n",
        label,
        count,
        resolve_p50,
        resolve_p95,
        max[prefix SUBSEP "prewarm_resolve"] + 0,
        max[prefix SUBSEP "prewarm_entries"] + 0,
        max[prefix SUBSEP "prewarm_deferred"] + 0,
        max[prefix SUBSEP "prewarm_over_budget"] + 0)
}

function gate_metric(gate, actual, label,    failed) {
    if (gate != "" && actual > gate) {
        printf("wgpu-frame-gate-fail metric=%s actual=%d gate=%d\n", label, actual, gate) > "/dev/stderr"
        return 1
    }
    return 0
}

/\[fika-wgpu\] frame=/ {
    view = value_of("view")
    if (view == "") {
        view = "unknown"
    }
    reason = value_of("reason")
    if (reason == "") {
        reason = "unknown"
    }
    prefix = "view:" view
    reason_prefix = "reason:" reason
    render = numeric_value("render")
    layout = numeric_value("layout")
    text_raster = numeric_value("text_raster")
    icon_resolve = numeric_value("icon_resolve")
    icon_raster = numeric_value("icon_raster")
    text_deferred = numeric_value("text_deferred")
    icon_deferred = numeric_value("icon_deferred")
    icon_raster_deferred = numeric_value("icon_raster_deferred")
    visible = numeric_value("visible")

    total_frames++
    frame_count["all"]++
    frame_count[prefix]++
    frame_count[reason_prefix]++
    view_seen[view] = 1
    reason_seen[reason] = 1

    warm = frame_count[prefix] > 1
    reason_warm = frame_count[reason_prefix] > 1
    add_render("all", render, total_frames > 1)
    add_render(prefix, render, warm)
    add_render(reason_prefix, render, reason_warm)

    bump_metric("all", "render", render)
    bump_metric(prefix, "render", render)
    bump_metric(reason_prefix, "render", render)
    if (total_frames > 1) {
        bump_metric("all", "warm_render", render)
    }
    if (warm) {
        bump_metric(prefix, "warm_render", render)
    }
    if (reason_warm) {
        bump_metric(reason_prefix, "warm_render", render)
    }

    for (target in targets) {
        # no-op placeholder so awk versions with strict parsers keep target local.
    }
    bump_metric("all", "layout", layout)
    bump_metric(prefix, "layout", layout)
    bump_metric(reason_prefix, "layout", layout)
    bump_metric("all", "text_raster", text_raster)
    bump_metric(prefix, "text_raster", text_raster)
    bump_metric(reason_prefix, "text_raster", text_raster)
    bump_metric("all", "icon_resolve", icon_resolve)
    bump_metric(prefix, "icon_resolve", icon_resolve)
    bump_metric(reason_prefix, "icon_resolve", icon_resolve)
    bump_metric("all", "icon_raster", icon_raster)
    bump_metric(prefix, "icon_raster", icon_raster)
    bump_metric(reason_prefix, "icon_raster", icon_raster)
    bump_metric("all", "text_deferred", text_deferred)
    bump_metric(prefix, "text_deferred", text_deferred)
    bump_metric(reason_prefix, "text_deferred", text_deferred)
    bump_metric("all", "icon_deferred", icon_deferred)
    bump_metric(prefix, "icon_deferred", icon_deferred)
    bump_metric(reason_prefix, "icon_deferred", icon_deferred)
    bump_metric("all", "icon_raster_deferred", icon_raster_deferred)
    bump_metric(prefix, "icon_raster_deferred", icon_raster_deferred)
    bump_metric(reason_prefix, "icon_raster_deferred", icon_raster_deferred)
    bump_metric("all", "visible", visible)
    bump_metric(prefix, "visible", visible)
    bump_metric(reason_prefix, "visible", visible)
}

/\[fika-wgpu\] prewarm-icons/ {
    view = value_of("view")
    if (view == "") {
        view = "unknown"
    }
    reason = value_of("reason")
    if (reason == "") {
        reason = "unknown"
    }
    prefix = "view:" view
    reason_prefix = "reason:" reason
    entries = numeric_value("entries")
    deferred = numeric_value("deferred")
    resolve = numeric_value("resolve")
    over_budget = numeric_value("over_budget")

    add_prewarm("all", resolve, entries, deferred, over_budget)
    add_prewarm(prefix, resolve, entries, deferred, over_budget)
    add_prewarm(reason_prefix, resolve, entries, deferred, over_budget)
    prewarm_view_seen[view] = 1
    prewarm_reason_seen[reason] = 1
}

END {
    failed = 0
    if (require_frames == "true" && total_frames == 0) {
        print "wgpu-frame-gate-fail metric=frames actual=0 gate=>0" > "/dev/stderr"
        exit 1
    }
    print_summary("all", "all")
    for (view in view_seen) {
        print_summary("view:" view, view)
    }
    for (reason in reason_seen) {
        print_summary("reason:" reason, "reason:" reason)
    }
    print_prewarm_summary("all", "all")
    for (view in prewarm_view_seen) {
        print_prewarm_summary("view:" view, view)
    }
    for (reason in prewarm_reason_seen) {
        print_prewarm_summary("reason:" reason, "reason:" reason)
    }

    failed += gate_metric(max_render_us, max["all" SUBSEP "render"] + 0, "render_us_max")
    failed += gate_metric(warm_max_render_us, max["all" SUBSEP "warm_render"] + 0, "warm_render_us_max")
    failed += gate_metric(warm_p95_render_us, percentile("all", "warm_render", 95), "warm_render_us_p95")
    failed += gate_metric(max_text_raster_us, max["all" SUBSEP "text_raster"] + 0, "text_raster_us_max")
    failed += gate_metric(max_icon_raster_us, max["all" SUBSEP "icon_raster"] + 0, "icon_raster_us_max")
    if (failed > 0) {
        exit 1
    }
}
' "$input_path"
