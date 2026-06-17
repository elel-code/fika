#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
analyzer="$root_dir/scripts/analyze-places-perf.sh"
tmpdir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

bash -n "$analyzer"

cat > "$tmpdir/complete.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-view] source=11 visible=11 sections=2 snapshot=4457us
[fika places-sidebar] rows=11 sections=2 elements=13 build=232us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=11 section_gpui=2 scrollbar_canvas=1
[fika autosmoke] places start scenario=DropTargets
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=89us
[fika places-sidebar] rows=11 sections=2 elements=13 build=186us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=11 section_gpui=2 scrollbar_canvas=1
[fika autosmoke] places snapshot=initial visible=11 sections=2 active=1 place_targets=0 insert_before=0 insert_after=0
[fika autosmoke] places action=target-first-place target=/home/yk changed=true
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=1 unchanged=12 removed=0 project=22us
[fika places-view] source=11 visible=11 sections=2 snapshot=110us
[fika places-sidebar] rows=11 sections=2 elements=13 build=220us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=11 section_gpui=2 scrollbar_canvas=1
[fika autosmoke] places snapshot=after-place-target visible=11 sections=2 active=1 place_targets=1 insert_before=0 insert_after=0
[fika autosmoke] places action=target-insert-start index=0 changed=true
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=1 unchanged=12 removed=0 project=40us
[fika places-view] source=11 visible=11 sections=2 snapshot=185us
[fika places-sidebar] rows=11 sections=2 elements=13 build=303us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=11 section_gpui=2 scrollbar_canvas=1
[fika autosmoke] places snapshot=after-insert-start visible=11 sections=2 active=1 place_targets=0 insert_before=1 insert_after=0
[fika autosmoke] places action=target-insert-end index=11 changed=true
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=2 unchanged=11 removed=0 project=30us
[fika places-view] source=11 visible=11 sections=2 snapshot=120us
[fika places-sidebar] rows=11 sections=2 elements=13 build=225us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=11 section_gpui=2 scrollbar_canvas=1
[fika autosmoke] places snapshot=after-insert-end visible=11 sections=2 active=1 place_targets=0 insert_before=0 insert_after=1
[fika autosmoke] places action=clear-targets changed=true
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=1 unchanged=12 removed=0 project=24us
[fika places-view] source=11 visible=11 sections=2 snapshot=110us
[fika places-sidebar] rows=11 sections=2 elements=13 build=224us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=11 section_gpui=2 scrollbar_canvas=1
[fika autosmoke] places snapshot=after-clear visible=11 sections=2 active=1 place_targets=0 insert_before=0 insert_after=0
[fika autosmoke] places complete scenario=DropTargets
EOF

summary="$("$analyzer" \
    --require-autosmoke \
    --expect-current-gpui-policy \
    --snapshot-us 5000 \
    --sidebar-build-us 1000 \
    --slot-project-us 100 \
    "$tmpdir/complete.log")"

if [[ "$summary" != *"places_view_frames="* || "$summary" != *"max_snapshot=4457us"* ]]; then
    echo "expected places-view summary" >&2
    exit 1
fi
if [[ "$summary" != *"places_slots_frames="* || "$summary" != *"max_visual=2"* ]]; then
    echo "expected places-slots summary" >&2
    exit 1
fi
if [[ "$summary" != *"places_autosmoke target=1 insert_start=1 insert_end=1 clear=1 snapshots=1,1,1,1,1"* ]]; then
    echo "expected Places autosmoke summary" >&2
    exit 1
fi
if [[ "$summary" != *"places_row_visual_frames=0"* ]]; then
    echo "expected default Places row visual summary" >&2
    exit 1
fi
if [[ "$summary" != *"places_scrollbar_frames=0"* ]]; then
    echo "expected default Places scrollbar summary" >&2
    exit 1
fi

cat > "$tmpdir/custom-row-visual.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 icon_gpui=11 retained_interaction=0 drag_shell=11 section_gpui=2 scrollbar_canvas=1
[fika places-row-visual] rows=11 prepaint=20us paint=31us
[fika places-row-shape-cache] hits=11 misses=0 evicted=0 entries=11
EOF

custom_summary="$("$analyzer" \
    --expect-custom-row-visual-policy \
    "$tmpdir/custom-row-visual.log")"

if [[ "$custom_summary" != *"max_row_gpui=0 max_row_visual_layer=11"* ]]; then
    echo "expected custom Places row visual policy summary" >&2
    exit 1
fi
if [[ "$custom_summary" != *"places_row_visual_frames=1 max_rows=11 max_prepaint=20us max_paint=31us"* ]]; then
    echo "expected Places row visual paint summary" >&2
    exit 1
fi
if [[ "$custom_summary" != *"places_row_shape_cache_frames=1 max_hits=11 max_misses=0 max_evicted=0 max_entries=11"* ]]; then
    echo "expected Places row shape-cache summary" >&2
    exit 1
fi

cat > "$tmpdir/custom-row-visual-per-row.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 icon_gpui=11 retained_interaction=0 drag_shell=11 section_gpui=2 scrollbar_canvas=1
[fika places-row-visual] rows=1 prepaint=20us paint=31us
[fika places-row-shape-cache] hits=11 misses=0 evicted=0 entries=11
EOF

if "$analyzer" --expect-custom-row-visual-policy "$tmpdir/custom-row-visual-per-row.log" >/dev/null 2>&1; then
    echo "expected per-row Places visual policy to fail" >&2
    exit 1
fi

cat > "$tmpdir/custom-row-visual-missing-shape-cache.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=240us
[fika places-renderer-policy] rows=11 row_gpui=0 row_visual_layer=11 icon_gpui=11 retained_interaction=0 drag_shell=11 section_gpui=2 scrollbar_canvas=1
[fika places-row-visual] rows=11 prepaint=20us paint=31us
EOF

if "$analyzer" --expect-custom-row-visual-policy "$tmpdir/custom-row-visual-missing-shape-cache.log" >/dev/null 2>&1; then
    echo "expected missing Places row shape-cache policy to fail" >&2
    exit 1
fi

cat > "$tmpdir/overflow.log" <<'EOF'
[fika places-slots] rows=75 sections=3 entries=78 inserted=78 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=90us
[fika places-view] source=11 visible=75 sections=3 snapshot=600us
[fika places-sidebar] rows=75 sections=3 elements=78 build=1400us
[fika places-renderer-policy] rows=75 row_gpui=75 row_visual_layer=0 icon_gpui=75 retained_interaction=0 drag_shell=75 section_gpui=3 scrollbar_canvas=1
[fika places-scrollbar] visible=1 max_scroll_y=1420 thumb_height=118 track_height=620
[fika autosmoke] places start scenario=Overflow
[fika places-slots] rows=75 sections=3 entries=78 inserted=0 content=0 geometry=0 visual=0 unchanged=78 removed=0 project=72us
[fika places-view] source=11 visible=75 sections=3 snapshot=510us
[fika autosmoke] places snapshot=overflow visible=75 sections=3 active=1 place_targets=0 insert_before=0 insert_after=0
[fika places-scrollbar] visible=1 max_scroll_y=1420 thumb_height=118 track_height=620
[fika autosmoke] places complete scenario=Overflow
EOF

overflow_summary="$("$analyzer" \
    --require-overflow-autosmoke \
    --expect-current-gpui-policy \
    "$tmpdir/overflow.log")"

if [[ "$overflow_summary" != *"places_scrollbar_frames=2 max_visible=1 max_scroll_y=1420.0"* ]]; then
    echo "expected Places overflow scrollbar summary" >&2
    exit 1
fi
if [[ "$overflow_summary" != *"places_overflow_autosmoke start=1 complete=1 snapshot=1 max_visible=75"* ]]; then
    echo "expected Places overflow autosmoke summary" >&2
    exit 1
fi

cat > "$tmpdir/missing-slots.log" <<'EOF'
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=11 section_gpui=2 scrollbar_canvas=1
EOF

if "$analyzer" "$tmpdir/missing-slots.log" >/dev/null 2>&1; then
    echo "expected missing places-slots channel to fail" >&2
    exit 1
fi

cat > "$tmpdir/no-unchanged.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=11 section_gpui=2 scrollbar_canvas=1
EOF

if "$analyzer" "$tmpdir/no-unchanged.log" >/dev/null 2>&1; then
    echo "expected missing unchanged slot frame to fail" >&2
    exit 1
fi

cat > "$tmpdir/bad-policy.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=10 row_visual_layer=1 icon_gpui=11 retained_interaction=1 drag_shell=11 section_gpui=2 scrollbar_canvas=1
EOF

if "$analyzer" --expect-current-gpui-policy "$tmpdir/bad-policy.log" >/dev/null 2>&1; then
    echo "expected invalid current GPUI policy to fail" >&2
    exit 1
fi

if "$analyzer" --expect-custom-row-visual-policy "$tmpdir/bad-policy.log" >/dev/null 2>&1; then
    echo "expected invalid custom row visual policy to fail" >&2
    exit 1
fi

cat > "$tmpdir/missing-autosmoke.log" <<'EOF'
[fika places-slots] rows=11 sections=2 entries=13 inserted=13 content=0 geometry=0 visual=0 unchanged=0 removed=0 project=25us
[fika places-slots] rows=11 sections=2 entries=13 inserted=0 content=0 geometry=0 visual=0 unchanged=13 removed=0 project=21us
[fika places-view] source=11 visible=11 sections=2 snapshot=100us
[fika places-sidebar] rows=11 sections=2 elements=13 build=200us
[fika places-renderer-policy] rows=11 row_gpui=11 row_visual_layer=0 icon_gpui=11 retained_interaction=0 drag_shell=11 section_gpui=2 scrollbar_canvas=1
EOF

if "$analyzer" --require-autosmoke "$tmpdir/missing-autosmoke.log" >/dev/null 2>&1; then
    echo "expected missing autosmoke markers to fail" >&2
    exit 1
fi

if "$analyzer" --require-overflow-autosmoke "$tmpdir/missing-autosmoke.log" >/dev/null 2>&1; then
    echo "expected missing overflow autosmoke markers to fail" >&2
    exit 1
fi

if "$analyzer" --snapshot-us 100 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected snapshot threshold violation to fail" >&2
    exit 1
fi

if "$analyzer" --sidebar-build-us 100 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected sidebar threshold violation to fail" >&2
    exit 1
fi

if "$analyzer" --slot-project-us 10 "$tmpdir/complete.log" >/dev/null 2>&1; then
    echo "expected slot threshold violation to fail" >&2
    exit 1
fi

: > "$tmpdir/empty.log"
if "$analyzer" "$tmpdir/empty.log" >/dev/null 2>&1; then
    echo "expected empty log to fail" >&2
    exit 1
fi

echo "places perf analyzer check passed"
