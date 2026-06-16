#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
analyzer="$root_dir/scripts/analyze-item-view-perf.sh"
tmpdir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

bash -n "$analyzer"

cat > "$tmpdir/complete.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us queue=1us convert=40us total=120us
[fika item-view] pane=1 mode=Icons phase=steady items=48 visible=40 raw=45us queue=1us convert=35us total=110us
[fika item-view] pane=1 mode=Details phase=steady items=48 visible=30 raw=42us queue=1us convert=32us total=105us
[fika file-grid] pane=1 mode=Compact visible=32 content=1602.5x882 build=400us
[fika file-grid] pane=1 mode=Icons visible=40 content=587x1168 build=450us
[fika file-grid] pane=1 mode=Details visible=30 content=601x882 build=420us
[fika static-item-visual] pane=1 mode=Compact prepaint_count=32 prepaint=180us paint_count=32 paint=160us
[fika static-item-visual] pane=1 mode=Icons prepaint_count=40 prepaint=210us paint_count=40 paint=190us
[fika item-image] pane=1 mode=Icons prepaint_count=8 prepaint=70us paint_count=8 paint=80us
[fika details-visual] pane=1 mode=Details prepaint_count=48 prepaint=120us paint_count=48 paint=130us
[fika details-shape-cache] pane=1 mode=Details hits=20 misses=2 evicted=0 entries=22
[fika item-interaction] pane=1 mode=Details prepaint_count=48 prepaint=60us paint_count=48 paint=50us
EOF

"$analyzer" \
    --require-steady \
    --require-details \
    --require-static-visual \
    --require-static-modes Compact,Icons \
    --require-interaction \
    --require-modes Compact,Icons,Details \
    --steady-total-us 1000 \
    --file-grid-build-us 3000 \
    --static-visual-paint-us 1000 \
    --image-paint-us 1000 \
    "$tmpdir/complete.log" >/dev/null

cat > "$tmpdir/missing-channels.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us queue=1us convert=40us total=120us
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
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us queue=1us convert=40us total=120us
[fika item-view] pane=1 mode=Icons phase=steady items=48 visible=40 raw=45us queue=1us convert=35us total=110us
[fika static-item-visual] pane=1 mode=Compact prepaint_count=32 prepaint=180us paint_count=32 paint=160us
EOF

if "$analyzer" --require-static-modes Compact,Icons "$tmpdir/missing-static-mode.log" >/dev/null 2>&1; then
    echo "expected missing required static visual mode to fail" >&2
    exit 1
fi

if "$analyzer" --require-modes Compact,Icons,Details "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected missing required modes to fail" >&2
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

: > "$tmpdir/empty.log"
if "$analyzer" "$tmpdir/empty.log" >/dev/null 2>&1; then
    echo "expected empty log to fail" >&2
    exit 1
fi

echo "item-view perf analyzer check passed"
