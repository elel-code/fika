#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
analyzer="$root_dir/scripts/analyze-item-view-perf.sh"
runtime_gate="$root_dir/scripts/check-item-view-runtime-log.sh"
renderer_evidence="$root_dir/scripts/summarize-item-view-renderer-evidence.sh"
tmpdir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

bash -n "$analyzer"
bash -n "$runtime_gate"
bash -n "$renderer_evidence"

cat > "$tmpdir/complete.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika item-view] pane=1 mode=Icons phase=steady items=48 visible=40 raw=45us icon_sync=3us queue=1us convert=35us total=110us
[fika item-view] pane=1 mode=Details phase=steady items=48 visible=30 raw=42us icon_sync=1us queue=1us convert=32us total=105us
[fika file-grid] pane=1 mode=Compact visible=32 content=1602.5x882 build=400us
[fika file-grid] pane=1 mode=Icons visible=40 content=587x1168 build=450us
[fika file-grid] pane=1 mode=Details visible=30 content=601x882 build=420us
[fika static-item-visual] pane=1 mode=Compact prepaint_count=32 prepaint=180us paint_count=32 paint=160us
[fika static-item-visual] pane=1 mode=Icons prepaint_count=40 prepaint=210us paint_count=40 paint=190us
[fika item-image] pane=1 mode=Icons prepaint_count=8 prepaint=70us paint_count=8 paint=80us theme_loaded=4 theme_decoded=2 theme_retained=1 theme_placeholder=1 thumb_loaded=2 thumb_decoded=1 thumb_retained=0 thumb_fallback=0
[fika details-visual] pane=1 mode=Details prepaint_count=48 prepaint=120us paint_count=48 paint=130us
[fika details-shape-cache] pane=1 mode=Details hits=20 misses=2 evicted=0 entries=22
[fika item-interaction] pane=1 mode=Details prepaint_count=48 prepaint=60us paint_count=48 paint=50us
[fika renderer-policy] pane=1 mode=Compact items=48 visual_layer=48 image_layer=8 retained_interaction=48 gpui_drag_shell=48 rename_overlay=0
[fika renderer-policy] pane=1 mode=Icons items=48 visual_layer=48 image_layer=8 retained_interaction=48 gpui_drag_shell=48 rename_overlay=0
[fika renderer-policy] pane=1 mode=Details items=48 visual_layer=48 image_layer=0 retained_interaction=48 gpui_drag_shell=48 rename_overlay=0
EOF

"$analyzer" \
    --require-steady \
    --require-details \
    --require-static-visual \
    --require-static-modes Compact,Icons \
    --require-interaction \
    --require-renderer-policy \
    --require-renderer-policy-modes Compact,Icons,Details \
    --require-modes Compact,Icons,Details \
    --steady-total-us 1000 \
    --file-grid-build-us 3000 \
    --static-visual-paint-us 1000 \
    --image-paint-us 1000 \
    --custom-paint-us 1000 \
    "$tmpdir/complete.log" >/dev/null

"$runtime_gate" "$tmpdir/complete.log" >/dev/null

evidence="$("$renderer_evidence" "$tmpdir/complete.log")"
if [[ "$evidence" != *"## Item View Renderer Evidence"* ]]; then
    echo "expected renderer evidence heading" >&2
    exit 1
fi
if [[ "$evidence" != *"custom_paint_frames"* ]]; then
    echo "expected renderer evidence to include analyzer summary" >&2
    exit 1
fi
if [[ "$evidence" != *"image_sources"* || "$evidence" != *"theme_placeholder=1"* ]]; then
    echo "expected renderer evidence to include image source summary" >&2
    exit 1
fi
if [[ "$evidence" != *"item_view_stage_max"* || "$evidence" != *"icon_sync=3us"* ]]; then
    echo "expected renderer evidence to include item-view stage summary" >&2
    exit 1
fi
if [[ "$evidence" != *"renderer_policy_frames"* ]]; then
    echo "expected renderer evidence to include renderer policy summary" >&2
    exit 1
fi

cat > "$tmpdir/legacy-no-icon-sync.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us queue=1us convert=40us total=120us
EOF

legacy_summary="$("$analyzer" "$tmpdir/legacy-no-icon-sync.log")"
if [[ "$legacy_summary" != *"item_view_stage_max"* || "$legacy_summary" != *"icon_sync=0us"* ]]; then
    echo "expected legacy item-view logs without icon_sync to stay parseable" >&2
    exit 1
fi

cat > "$tmpdir/missing-channels.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
EOF

if "$analyzer" --require-details --require-interaction "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected missing details/interaction channels to fail" >&2
    exit 1
fi

if "$analyzer" --require-static-visual "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected missing static visual channel to fail" >&2
    exit 1
fi

cat > "$tmpdir/missing-static-mode.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika item-view] pane=1 mode=Icons phase=steady items=48 visible=40 raw=45us icon_sync=3us queue=1us convert=35us total=110us
[fika static-item-visual] pane=1 mode=Compact prepaint_count=32 prepaint=180us paint_count=32 paint=160us
EOF

if "$analyzer" --require-static-modes Compact,Icons "$tmpdir/missing-static-mode.log" >/dev/null 2>&1; then
    echo "expected missing required static visual mode to fail" >&2
    exit 1
fi

cat > "$tmpdir/missing-renderer-policy-mode.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika item-view] pane=1 mode=Icons phase=steady items=48 visible=40 raw=45us icon_sync=3us queue=1us convert=35us total=110us
[fika item-view] pane=1 mode=Details phase=steady items=48 visible=30 raw=42us icon_sync=1us queue=1us convert=32us total=105us
[fika renderer-policy] pane=1 mode=Compact items=48 visual_layer=48 image_layer=8 retained_interaction=48 gpui_drag_shell=48 rename_overlay=0
[fika renderer-policy] pane=1 mode=Icons items=48 visual_layer=48 image_layer=8 retained_interaction=48 gpui_drag_shell=48 rename_overlay=0
EOF

if "$analyzer" --require-renderer-policy-modes Compact,Icons,Details "$tmpdir/missing-renderer-policy-mode.log" >/dev/null 2>&1; then
    echo "expected missing required renderer-policy mode to fail" >&2
    exit 1
fi

cat > "$tmpdir/invalid-renderer-policy-count.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=2 visible=2 raw=50us icon_sync=2us queue=1us convert=40us total=120us
[fika renderer-policy] pane=1 mode=Compact items=2 visual_layer=3 image_layer=0 retained_interaction=2 gpui_drag_shell=2 rename_overlay=0
EOF

if "$analyzer" "$tmpdir/invalid-renderer-policy-count.log" >/dev/null 2>&1; then
    echo "expected invalid renderer-policy surface count to fail" >&2
    exit 1
fi

if "$analyzer" --require-modes Compact,Icons,Details "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected missing required modes to fail" >&2
    exit 1
fi

if "$runtime_gate" "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected runtime log gate to fail missing channels" >&2
    exit 1
fi

if "$renderer_evidence" "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected renderer evidence to fail missing channels" >&2
    exit 1
fi

if "$analyzer" --steady-total-us 100 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected steady threshold violation to fail" >&2
    exit 1
fi

if "$analyzer" --static-visual-paint-us 100 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected static visual paint threshold violation to fail" >&2
    exit 1
fi

if "$analyzer" --image-paint-us 50 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected item image paint threshold violation to fail" >&2
    exit 1
fi

if "$analyzer" --custom-paint-us 100 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected custom paint threshold violation to fail" >&2
    exit 1
fi

: > "$tmpdir/empty.log"
if "$analyzer" "$tmpdir/empty.log" >/dev/null 2>&1; then
    echo "expected empty log to fail" >&2
    exit 1
fi

echo "item-view perf analyzer check passed"
