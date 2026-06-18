# Retained Renderer Evidence Checklist

Use this checklist before changing a default renderer policy or removing a GPUI
bridge. It turns Track 1 of `docs/FULL_RETAINED_RENDERER_ROADMAP.md` into a
repeatable desktop-session procedure.

Run GUI commands from a real desktop session. A sandbox or headless shell can
return GPUI `NoCompositor`, which is not valid runtime evidence. Prefer a built
binary over `cargo run` so compile time is not mixed into the log.

The standard core capture is automated:

```sh
scripts/run-retained-renderer-evidence.sh --core
```

When a slice only changes one side of the renderer boundary, use a narrower
capture so unrelated evidence is not mixed into the review:

```sh
scripts/run-retained-renderer-evidence.sh --items-only
scripts/run-retained-renderer-evidence.sh --places-only
```

Use `--icons` only when validating a MIME/theme icon renderer candidate that is
expected to pass default-promotion gates:

```sh
scripts/run-retained-renderer-evidence.sh --icons
```

Use `--hybrid-icons` when validating the staged GPUI-to-custom readiness
handoff path:

```sh
scripts/run-retained-renderer-evidence.sh --hybrid-icons
```

The sections below show the commands that the script runs and the manual checks
that still need human review.

## Build

```sh
cargo build
```

## Item View Baseline

Capture default item-view logs for a mixed user directory and `/etc`:

```sh
timeout 8s env FIKA_PERF_ITEM_VIEW=1 target/debug/fika ~/Downloads > /tmp/fika-evidence-item-downloads.log 2>&1
timeout 8s env FIKA_PERF_ITEM_VIEW=1 target/debug/fika /etc > /tmp/fika-evidence-item-etc.log 2>&1
```

Capture unattended zoom/scroll evidence for `/etc`:

```sh
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-evidence-item-etc-zoom-scroll.log 2>&1
```

Analyze at least the most complete log:

```sh
scripts/check-item-view-runtime-log.sh /tmp/fika-evidence-item-etc-zoom-scroll.log
scripts/summarize-item-view-renderer-evidence.sh /tmp/fika-evidence-item-etc-zoom-scroll.log
```

The summary block is the preferred evidence snippet for
`docs/ITEM_VIEW_RENDERER_DECISIONS.md`.

## MIME/Theme Icon A/B

Only required when changing MIME/theme icon rendering:

```sh
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_GPUI_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-evidence-icon-default-etc.log 2>&1
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_CUSTOM_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-evidence-icon-custom-etc.log 2>&1
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_GPUI_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-evidence-icon-default-downloads.log 2>&1
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_CUSTOM_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-evidence-icon-custom-downloads.log 2>&1
```

Analyze paired logs:

```sh
scripts/compare-item-image-renderers.sh --gate-default-promotion /tmp/fika-evidence-icon-custom-etc.log /tmp/fika-evidence-icon-default-etc.log
scripts/compare-item-image-renderers.sh --gate-default-promotion /tmp/fika-evidence-icon-custom-downloads.log /tmp/fika-evidence-icon-default-downloads.log
```

For the staged hybrid readiness path, use:

```sh
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_GPUI_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-evidence-icon-hybrid-default-etc.log 2>&1
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-evidence-icon-hybrid-etc.log 2>&1
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_GPUI_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-evidence-icon-hybrid-default-downloads.log 2>&1
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-evidence-icon-hybrid-downloads.log 2>&1

scripts/compare-item-image-renderers.sh --gate-hybrid-handoff /tmp/fika-evidence-icon-hybrid-etc.log /tmp/fika-evidence-icon-hybrid-default-etc.log
scripts/compare-item-image-renderers.sh --gate-hybrid-handoff /tmp/fika-evidence-icon-hybrid-downloads.log /tmp/fika-evidence-icon-hybrid-default-downloads.log
scripts/compare-item-image-renderers.sh --gate-hybrid-default-promotion /tmp/fika-evidence-icon-hybrid-etc.log /tmp/fika-evidence-icon-hybrid-default-etc.log
scripts/compare-item-image-renderers.sh --gate-hybrid-default-promotion /tmp/fika-evidence-icon-hybrid-downloads.log /tmp/fika-evidence-icon-hybrid-default-downloads.log
```

A default-promotion candidate must have no visible `theme_placeholder` churn, no
zoom-time `theme_decoded` burst, no visible icon-size second jump, and no
sync icon-work regression. Hybrid default-promotion candidates must additionally
stay within the compare script's explicit phase, static-visual, image-paint, and
icon-sync tolerances versus the default GPUI image-element baseline.

## Places Baseline

Capture the default Places chrome retained-DnD policy:

```sh
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-evidence-places-targets.log 2>&1
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-evidence-places-overflow.log 2>&1
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=layout target/debug/fika /etc > /tmp/fika-evidence-places-layout.log 2>&1
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=hit-test target/debug/fika /etc > /tmp/fika-evidence-places-hit-test.log 2>&1
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targeting target/debug/fika /etc > /tmp/fika-evidence-places-targeting.log 2>&1
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=dnd target/debug/fika /etc > /tmp/fika-evidence-places-dnd.log 2>&1
```

Analyze the logs:

```sh
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-evidence-places-targets.log
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-evidence-places-overflow.log
scripts/analyze-places-perf.sh --require-layout-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-evidence-places-layout.log
scripts/analyze-places-perf.sh --require-hit-test-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-evidence-places-hit-test.log
scripts/analyze-places-perf.sh --require-retained-targeting-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-evidence-places-targeting.log
scripts/analyze-places-perf.sh --require-retained-dnd-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-evidence-places-dnd.log
```

For the current default retained-DnD mixed policy, the dnd summary should show:

```text
max_gpui_row_section_event_shells=0
max_gpui_typed_dnd_payload_shells=1
max_gpui_sidebar_leave_shells=0
```

The full retained-event gate should still fail until the typed payload shell is
removed through Track 4:

```sh
scripts/analyze-places-perf.sh --expect-retained-event-policy /tmp/fika-evidence-places-dnd.log
```

## Manual Smoke Still Required

Perf logs do not replace behavior review for:

- pane item drag to pane directory
- pane item drag to Places
- Places drag to pane directory
- external path drop
- rename focus, selection, validation, commit, cancel, and IME behavior

Record any manual DnD trace with `FIKA_DEBUG_DND=1`. A valid pane self-drag may
show `active-item-move via=preview`; the required signal is that the retained
hit test reaches `kind=Some(Directory)` and the directory highlights before
drop.

## Recording Rule

When a renderer policy changes, the owning plan or decision document must record:

- log paths under `/tmp`
- analyzer commands that passed or intentionally failed
- symptom and root cause
- Dolphin comparison point
- implementation boundary
- future regression guard

Do not promote a custom renderer or remove a GPUI bridge using only unit tests
or architecture preference.
