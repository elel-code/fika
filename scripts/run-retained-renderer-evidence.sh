#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: run-retained-renderer-evidence.sh [OPTIONS]

Captures and analyzes the retained-renderer Track 1 runtime evidence described
in docs/RETAINED_RENDERER_EVIDENCE_CHECKLIST.md. Run this from a real desktop
session; headless/sandbox sessions can fail with GPUI NoCompositor and are not
valid runtime evidence.

Options:
  --core
      Capture item-view and Places evidence. This is the default.

  --items-only
      Capture only item-view evidence.

  --places-only
      Capture only Places evidence.

  --icons
      Also capture MIME/theme icon default-vs-custom A/B logs and require the
      default-promotion gate to pass. Use this only when testing an image
      renderer candidate that is expected to be promotable.

  --hybrid-icons
      Capture MIME/theme icon default-vs-hybrid readiness handoff logs and
      require the hybrid default-promotion gate to pass.

  --places-full-handoff
      Capture paired default-chrome vs full Places ready-only handoff logs for
      targets, overflow, and layout. This is a promotion-evidence input only;
      it does not make full Places rows the default.

  --all
      Same as --core --icons --hybrid-icons --places-full-handoff.

      Note: --icons is intentionally strict and may fail for the current
      non-promotable full custom theme-icon path. Use --hybrid-icons by itself
      when validating staged readiness handoff work.

  --analyze-only
      Do not launch Fika. Re-run analyzers against existing logs.

  --skip-build
      Do not run cargo build before launching Fika.

  --binary PATH
      Fika binary to run. Default: target/debug/fika.

  --out-dir DIR
      Directory for logs. Default: /tmp.

  --prefix NAME
      Log filename prefix. Default: fika-evidence.

  --downloads DIR
      Mixed user directory for item-view evidence. Default: $HOME/Downloads.

  --timeout SECONDS
      Timeout for each GUI capture. Default: 8.

  -h, --help
      Show this help.
EOF
}

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$root_dir/target/debug/fika"
out_dir="/tmp"
prefix="fika-evidence"
downloads_dir="${HOME:-}/Downloads"
timeout_seconds=8
capture_items=false
capture_places=false
capture_icons=false
capture_hybrid_icons=false
capture_places_full_handoff=false
analyze_only=false
skip_build=false
explicit_selection=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --core)
            explicit_selection=true
            capture_items=true
            capture_places=true
            ;;
        --items-only)
            explicit_selection=true
            capture_items=true
            capture_places=false
            ;;
        --places-only)
            explicit_selection=true
            capture_items=false
            capture_places=true
            ;;
        --icons)
            explicit_selection=true
            capture_icons=true
            ;;
        --hybrid-icons)
            explicit_selection=true
            capture_hybrid_icons=true
            ;;
        --places-full-handoff)
            explicit_selection=true
            capture_places_full_handoff=true
            ;;
        --all)
            explicit_selection=true
            capture_items=true
            capture_places=true
            capture_icons=true
            capture_hybrid_icons=true
            capture_places_full_handoff=true
            ;;
        --analyze-only)
            analyze_only=true
            ;;
        --skip-build)
            skip_build=true
            ;;
        --binary)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--binary requires a path" >&2
                usage >&2
                exit 2
            fi
            binary="$2"
            shift
            ;;
        --binary=*)
            binary="${1#--binary=}"
            ;;
        --out-dir)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--out-dir requires a path" >&2
                usage >&2
                exit 2
            fi
            out_dir="$2"
            shift
            ;;
        --out-dir=*)
            out_dir="${1#--out-dir=}"
            ;;
        --prefix)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--prefix requires a name" >&2
                usage >&2
                exit 2
            fi
            prefix="$2"
            shift
            ;;
        --prefix=*)
            prefix="${1#--prefix=}"
            ;;
        --downloads)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--downloads requires a directory" >&2
                usage >&2
                exit 2
            fi
            downloads_dir="$2"
            shift
            ;;
        --downloads=*)
            downloads_dir="${1#--downloads=}"
            ;;
        --timeout)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--timeout requires a number of seconds" >&2
                usage >&2
                exit 2
            fi
            timeout_seconds="$2"
            shift
            ;;
        --timeout=*)
            timeout_seconds="${1#--timeout=}"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
    shift
done

if [[ "$explicit_selection" != true ]]; then
    capture_items=true
    capture_places=true
fi

if [[ "$analyze_only" != true && "$skip_build" != true ]]; then
    (cd "$root_dir" && cargo build)
fi

mkdir -p "$out_dir"

log_path() {
    printf '%s/%s-%s.log' "$out_dir" "$prefix" "$1"
}

run_capture() {
    local label="$1"
    local log="$2"
    shift 2

    if [[ "$analyze_only" == true ]]; then
        if [[ ! -s "$log" ]]; then
            echo "missing log for --analyze-only: $log" >&2
            exit 1
        fi
        return
    fi

    echo "capture: $label -> $log"
    set +e
    timeout "${timeout_seconds}s" "$@" > "$log" 2>&1
    local status=$?
    set -e

    if [[ $status -ne 0 && $status -ne 124 ]]; then
        echo "capture failed ($status): $label" >&2
        tail -80 "$log" >&2 || true
        exit "$status"
    fi
    if grep -q "NoCompositor" "$log"; then
        echo "capture is not valid desktop-session evidence: $label hit GPUI NoCompositor" >&2
        tail -80 "$log" >&2 || true
        exit 1
    fi
}

run_gate() {
    local label="$1"
    shift
    echo "analyze: $label"
    "$@"
}

expect_gate_failure() {
    local label="$1"
    shift
    echo "analyze expected-fail: $label"
    set +e
    "$@" >/tmp/fika-retained-renderer-expected-fail.out 2>/tmp/fika-retained-renderer-expected-fail.err
    local status=$?
    set -e
    if [[ $status -eq 0 ]]; then
        echo "expected analyzer failure but command passed: $label" >&2
        cat /tmp/fika-retained-renderer-expected-fail.out >&2
        exit 1
    fi
}

if [[ "$capture_items" == true ]]; then
    item_downloads_log="$(log_path item-downloads)"
    item_etc_log="$(log_path item-etc)"
    item_zoom_log="$(log_path item-etc-zoom-scroll)"
    item_details_log="$(log_path item-etc-details-zoom-scroll)"

    run_capture "item downloads" "$item_downloads_log" \
        env FIKA_PERF_ITEM_VIEW=1 "$binary" "$downloads_dir"
    run_capture "item etc" "$item_etc_log" \
        env FIKA_PERF_ITEM_VIEW=1 "$binary" /etc
    run_capture "item etc zoom-scroll" "$item_zoom_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll "$binary" /etc
    run_capture "item etc details zoom-scroll" "$item_details_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=details-zoom-scroll "$binary" /etc

    run_gate "item runtime" \
        "$root_dir/scripts/check-item-view-runtime-log.sh" "$item_zoom_log"
    run_gate "item renderer evidence summary" \
        "$root_dir/scripts/summarize-item-view-renderer-evidence.sh" "$item_zoom_log"
    run_gate "item details renderer policy" \
        "$root_dir/scripts/analyze-item-view-perf.sh" \
        --require-autosmoke \
        --require-details \
        --require-renderer-policy \
        --require-interaction \
        --expect-retained-item-policy \
        --require-modes Details \
        --require-renderer-policy-modes Details \
        "$item_details_log"
fi

if [[ "$capture_places" == true ]]; then
    places_targets_log="$(log_path places-targets)"
    places_overflow_log="$(log_path places-overflow)"
    places_layout_log="$(log_path places-layout)"
    places_hit_test_log="$(log_path places-hit-test)"
    places_targeting_log="$(log_path places-targeting)"
    places_dnd_log="$(log_path places-dnd)"

    run_capture "places targets" "$places_targets_log" \
        env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targets "$binary" /etc
    run_capture "places overflow" "$places_overflow_log" \
        env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=overflow "$binary" /etc
    run_capture "places layout" "$places_layout_log" \
        env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=layout "$binary" /etc
    run_capture "places hit-test" "$places_hit_test_log" \
        env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=hit-test "$binary" /etc
    run_capture "places targeting" "$places_targeting_log" \
        env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targeting "$binary" /etc
    run_capture "places dnd" "$places_dnd_log" \
        env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=dnd "$binary" /etc

    places_analyzer="$root_dir/scripts/analyze-places-perf.sh"
    places_common=(--require-interaction-policy --require-interaction-geometry --expect-custom-row-full-policy)
    run_gate "places targets" "$places_analyzer" --require-autosmoke "${places_common[@]}" "$places_targets_log"
    run_gate "places overflow" "$places_analyzer" --require-overflow-autosmoke "${places_common[@]}" "$places_overflow_log"
    run_gate "places layout" "$places_analyzer" --require-layout-autosmoke "${places_common[@]}" "$places_layout_log"
    run_gate "places hit-test" "$places_analyzer" --require-hit-test-autosmoke "${places_common[@]}" "$places_hit_test_log"
    run_gate "places targeting" "$places_analyzer" --require-retained-targeting-autosmoke "${places_common[@]}" "$places_targeting_log"
    run_gate "places dnd" "$places_analyzer" --require-retained-dnd-autosmoke "${places_common[@]}" "$places_dnd_log"
    expect_gate_failure "places full retained-event still blocked by typed payload shell" \
        "$places_analyzer" --expect-retained-event-policy "$places_dnd_log"
fi

if [[ "$capture_icons" == true ]]; then
    icon_default_etc_log="$(log_path icon-default-etc)"
    icon_custom_etc_log="$(log_path icon-custom-etc)"
    icon_default_downloads_log="$(log_path icon-default-downloads)"
    icon_custom_downloads_log="$(log_path icon-custom-downloads)"

    run_capture "icon default etc" "$icon_default_etc_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_GPUI_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll "$binary" /etc
    run_capture "icon custom etc" "$icon_custom_etc_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_CUSTOM_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll "$binary" /etc
    run_capture "icon default downloads" "$icon_default_downloads_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_GPUI_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll "$binary" "$downloads_dir"
    run_capture "icon custom downloads" "$icon_custom_downloads_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_CUSTOM_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll "$binary" "$downloads_dir"

    compare="$root_dir/scripts/compare-item-image-renderers.sh"
    run_gate "icon default promotion etc" \
        "$compare" --gate-default-promotion "$icon_custom_etc_log" "$icon_default_etc_log"
    run_gate "icon default promotion downloads" \
        "$compare" --gate-default-promotion "$icon_custom_downloads_log" "$icon_default_downloads_log"
fi

if [[ "$capture_hybrid_icons" == true ]]; then
    icon_default_etc_log="$(log_path icon-hybrid-default-etc)"
    icon_hybrid_etc_log="$(log_path icon-hybrid-etc)"
    icon_default_downloads_log="$(log_path icon-hybrid-default-downloads)"
    icon_hybrid_downloads_log="$(log_path icon-hybrid-downloads)"

    run_capture "icon hybrid gpui baseline etc" "$icon_default_etc_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_GPUI_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll "$binary" /etc
    run_capture "icon hybrid etc" "$icon_hybrid_etc_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll "$binary" /etc
    run_capture "icon hybrid gpui baseline downloads" "$icon_default_downloads_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_GPUI_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll "$binary" "$downloads_dir"
    run_capture "icon hybrid downloads" "$icon_hybrid_downloads_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll "$binary" "$downloads_dir"

    compare="$root_dir/scripts/compare-item-image-renderers.sh"
    run_gate "icon hybrid default promotion etc" \
        "$compare" --gate-hybrid-default-promotion "$icon_hybrid_etc_log" "$icon_default_etc_log"
    run_gate "icon hybrid default promotion downloads" \
        "$compare" --gate-hybrid-default-promotion "$icon_hybrid_downloads_log" "$icon_default_downloads_log"
fi

if [[ "$capture_places_full_handoff" == true ]]; then
    places_analyzer="$root_dir/scripts/analyze-places-perf.sh"
    places_handoff_chrome_targets_log="$(log_path places-handoff-chrome-targets)"
    places_handoff_full_targets_log="$(log_path places-handoff-full-targets)"
    places_handoff_chrome_overflow_log="$(log_path places-handoff-chrome-overflow)"
    places_handoff_full_overflow_log="$(log_path places-handoff-full-overflow)"
    places_handoff_chrome_layout_log="$(log_path places-handoff-chrome-layout)"
    places_handoff_full_layout_log="$(log_path places-handoff-full-layout)"

    run_capture "places handoff chrome targets" "$places_handoff_chrome_targets_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targets "$binary" /etc
    run_capture "places handoff full targets" "$places_handoff_full_targets_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_PERF_PLACES_VIEW=1 FIKA_PLACES_ROW_VISUAL_POLICY=full FIKA_PLACES_ROW_VISUAL_HANDOFF=1 FIKA_AUTOSMOKE_PLACES=targets "$binary" /etc
    run_capture "places handoff chrome overflow" "$places_handoff_chrome_overflow_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=overflow "$binary" /etc
    run_capture "places handoff full overflow" "$places_handoff_full_overflow_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_PERF_PLACES_VIEW=1 FIKA_PLACES_ROW_VISUAL_POLICY=full FIKA_PLACES_ROW_VISUAL_HANDOFF=1 FIKA_AUTOSMOKE_PLACES=overflow "$binary" /etc
    run_capture "places handoff chrome layout" "$places_handoff_chrome_layout_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=layout "$binary" /etc
    run_capture "places handoff full layout" "$places_handoff_full_layout_log" \
        env FIKA_PERF_ITEM_VIEW=1 FIKA_PERF_PLACES_VIEW=1 FIKA_PLACES_ROW_VISUAL_POLICY=full FIKA_PLACES_ROW_VISUAL_HANDOFF=1 FIKA_AUTOSMOKE_PLACES=layout "$binary" /etc

    places_handoff_chrome_common=(--require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy --render-total-us 25000)
    places_handoff_full_common=(--require-interaction-policy --require-interaction-geometry --expect-custom-row-handoff-policy --row-visual-prepaint-us 1000 --row-visual-paint-us 3000 --row-visual-warm-prepaint-us 500 --row-visual-warm-paint-us 3000 --render-total-us 30000)

    run_gate "places handoff chrome targets" \
        "$places_analyzer" --require-autosmoke "${places_handoff_chrome_common[@]}" "$places_handoff_chrome_targets_log"
    run_gate "places handoff full targets" \
        "$places_analyzer" --require-autosmoke "${places_handoff_full_common[@]}" "$places_handoff_full_targets_log"
    run_gate "places handoff chrome overflow" \
        "$places_analyzer" --require-overflow-autosmoke "${places_handoff_chrome_common[@]}" "$places_handoff_chrome_overflow_log"
    run_gate "places handoff full overflow" \
        "$places_analyzer" --require-overflow-autosmoke "${places_handoff_full_common[@]}" "$places_handoff_full_overflow_log"
    run_gate "places handoff chrome layout" \
        "$places_analyzer" --require-layout-autosmoke "${places_handoff_chrome_common[@]}" "$places_handoff_chrome_layout_log"
    run_gate "places handoff full layout" \
        "$places_analyzer" --require-layout-autosmoke "${places_handoff_full_common[@]}" "$places_handoff_full_layout_log"
fi

echo "retained renderer evidence complete"
