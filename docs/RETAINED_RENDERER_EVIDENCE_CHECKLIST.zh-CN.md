> 本文是 [RETAINED_RENDERER_EVIDENCE_CHECKLIST.md](RETAINED_RENDERER_EVIDENCE_CHECKLIST.md)
> 的简体中文翻译。

# Retained Renderer 证据清单

在更改默认 renderer policy 或移除 GPUI bridge 前使用这份清单。它把
`docs/FULL_RETAINED_RENDERER_ROADMAP.md` 的 Track 1 变成可重复执行的桌面会话流程。

GUI 命令必须在真实桌面会话中运行。沙箱或 headless shell 可能返回 GPUI
`NoCompositor`，这种结果不是有效运行时证据。优先使用已构建的 binary，而不是
`cargo run`，避免编译时间混入日志。

标准 core 采集已自动化：

```sh
scripts/run-retained-renderer-evidence.sh --core
```

当某个切片只改变 renderer 边界的一侧时，使用更窄的采集，避免混入无关证据：

```sh
scripts/run-retained-renderer-evidence.sh --items-only
scripts/run-retained-renderer-evidence.sh --places-only
```

只有在验证预期能通过默认提升 gate 的 MIME/theme icon renderer 候选时，才使用
`--icons`：

```sh
scripts/run-retained-renderer-evidence.sh --icons
```

验证分阶段 GPUI 到 custom readiness handoff 路径时，使用 `--hybrid-icons`：

```sh
scripts/run-retained-renderer-evidence.sh --hybrid-icons
```

下面各节展示脚本运行的命令，以及仍需人工审查的手动检查。

## 构建

```sh
cargo build
```

## Item View 基线

为混合用户目录和 `/etc` 采集默认 item-view 日志：

```sh
timeout 8s env FIKA_PERF_ITEM_VIEW=1 target/debug/fika ~/Downloads > /tmp/fika-evidence-item-downloads.log 2>&1
timeout 8s env FIKA_PERF_ITEM_VIEW=1 target/debug/fika /etc > /tmp/fika-evidence-item-etc.log 2>&1
```

为 `/etc` 采集无人值守 zoom/scroll 证据：

```sh
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-evidence-item-etc-zoom-scroll.log 2>&1
```

至少分析最完整的日志：

```sh
scripts/check-item-view-runtime-log.sh /tmp/fika-evidence-item-etc-zoom-scroll.log
scripts/summarize-item-view-renderer-evidence.sh /tmp/fika-evidence-item-etc-zoom-scroll.log
```

summary block 是写入 `docs/ITEM_VIEW_RENDERER_DECISIONS.md` 的首选证据片段。

## MIME/Theme Icon A/B

仅在更改 MIME/theme icon renderer 时需要：

```sh
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_GPUI_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-evidence-icon-default-etc.log 2>&1
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_CUSTOM_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-evidence-icon-custom-etc.log 2>&1
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_GPUI_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-evidence-icon-default-downloads.log 2>&1
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_CUSTOM_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-evidence-icon-custom-downloads.log 2>&1
```

分析配对日志：

```sh
scripts/compare-item-image-renderers.sh --gate-default-promotion /tmp/fika-evidence-icon-custom-etc.log /tmp/fika-evidence-icon-default-etc.log
scripts/compare-item-image-renderers.sh --gate-default-promotion /tmp/fika-evidence-icon-custom-downloads.log /tmp/fika-evidence-icon-default-downloads.log
```

对于分阶段 hybrid readiness 路径，使用：

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

默认提升候选必须没有可见 `theme_placeholder` 抖动、没有 zoom-time `theme_decoded`
burst、没有可见图标尺寸二次跳变，并且没有同步 icon work 回归。Hybrid 默认提升候选还必须在
compare 脚本针对 phase、static visual、image paint 和 icon_sync 定义的显式容差内，不弱于默认
GPUI image-element baseline。

## Places 基线

采集默认 Places chrome retained-DnD policy：

```sh
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-evidence-places-targets.log 2>&1
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-evidence-places-overflow.log 2>&1
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=layout target/debug/fika /etc > /tmp/fika-evidence-places-layout.log 2>&1
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=hit-test target/debug/fika /etc > /tmp/fika-evidence-places-hit-test.log 2>&1
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targeting target/debug/fika /etc > /tmp/fika-evidence-places-targeting.log 2>&1
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=dnd target/debug/fika /etc > /tmp/fika-evidence-places-dnd.log 2>&1
```

分析日志：

```sh
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-evidence-places-targets.log
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-evidence-places-overflow.log
scripts/analyze-places-perf.sh --require-layout-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-evidence-places-layout.log
scripts/analyze-places-perf.sh --require-hit-test-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-evidence-places-hit-test.log
scripts/analyze-places-perf.sh --require-retained-targeting-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-evidence-places-targeting.log
scripts/analyze-places-perf.sh --require-retained-dnd-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-evidence-places-dnd.log
```

当前默认 retained-DnD mixed policy 的 dnd summary 应显示：

```text
max_gpui_row_section_event_shells=0
max_gpui_typed_dnd_payload_shells=1
max_gpui_sidebar_leave_shells=0
```

在 Track 4 移除 typed payload shell 前，完整 retained-event gate 应继续失败：

```sh
scripts/analyze-places-perf.sh --expect-retained-event-policy /tmp/fika-evidence-places-dnd.log
```

## 仍需手动 Smoke

Perf 日志不能替代以下行为审查：

- pane item 拖到 pane 目录
- pane item 拖到 Places
- Places 拖到 pane 目录
- 外部路径 drop
- rename focus、selection、validation、commit、cancel 和 IME 行为

用 `FIKA_DEBUG_DND=1` 记录手动 DnD trace。有效 pane self-drag 可以只显示
`active-item-move via=preview`；必需信号是 retained hit test 到达
`kind=Some(Directory)`，且 drop 前目录高亮。

## 记录规则

当 renderer policy 变化时，所属 plan 或 decision 文档必须记录：

- `/tmp` 下的日志路径
- 通过或按预期失败的 analyzer 命令
- 症状和根因
- Dolphin 对比点
- 实现边界
- 未来回归守卫

不要只用单元测试或架构偏好来提升 custom renderer 或移除 GPUI bridge。
