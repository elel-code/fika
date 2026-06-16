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
[fika file-grid] pane=1 mode=Compact visible=32 content=1602.5x882 build=400us
[fika details-visual] pane=1 mode=Details prepaint_count=48 prepaint=120us paint_count=48 paint=130us
[fika details-shape-cache] pane=1 mode=Details hits=20 misses=2 evicted=0 entries=22
[fika item-interaction] pane=1 mode=Details prepaint_count=48 prepaint=60us paint_count=48 paint=50us
EOF

"$analyzer" \
    --require-steady \
    --require-details \
    --require-interaction \
    --steady-total-us 1000 \
    --file-grid-build-us 3000 \
    "$tmpdir/complete.log" >/dev/null

cat > "$tmpdir/missing-channels.log" <<'EOF'
[fika item-view] pane=1 mode=Compact phase=steady items=48 visible=32 raw=50us queue=1us convert=40us total=120us
EOF

if "$analyzer" --require-details --require-interaction "$tmpdir/missing-channels.log" >/dev/null 2>&1; then
    echo "expected missing details/interaction channels to fail" >&2
    exit 1
fi

if "$analyzer" --steady-total-us 100 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected steady threshold violation to fail" >&2
    exit 1
fi

: > "$tmpdir/empty.log"
if "$analyzer" "$tmpdir/empty.log" >/dev/null 2>&1; then
    echo "expected empty log to fail" >&2
    exit 1
fi

echo "item-view perf analyzer check passed"
