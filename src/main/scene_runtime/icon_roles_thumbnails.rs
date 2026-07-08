impl ShellScene {

    fn enqueue_dolphin_small_directory_icon_roles(
        &self,
        projections: &[ShellPaneProjection<'_>],
    ) -> bool {
        let mut queued = false;
        for projection in projections {
            if projection.view.filtered_entry_count() > DOLPHIN_RESOLVE_ALL_ITEMS_LIMIT {
                continue;
            }
            let Some(icon_size) = projection.visible_items.first().map(|item| {
                item.layout
                    .icon_rect
                    .width
                    .max(item.layout.icon_rect.height)
                    .clamp(16.0, 256.0)
            }) else {
                continue;
            };
            for entry_index in projection.view.filtered_indexes.iter().copied() {
                let Some(entry) = projection.view.entries.get(entry_index) else {
                    continue;
                };
                self.enqueue_icon_role_read_ahead(projection.view.path, entry, icon_size);
                queued = true;
            }
        }
        queued
    }

    fn enqueue_icon_role_read_ahead(&self, directory: &Path, entry: &Entry, icon_size: f32) {
        let path = directory.join(entry.name.as_ref());
        let key = file_icon_path_cache_key(
            &path,
            entry.is_dir,
            entry.mime_type.clone(),
            entry.mime_magic_checked,
            icon_size,
        );
        self.icon_role_read_ahead.borrow_mut().push_key(key);
    }

    fn resolve_next_icon_role_read_ahead(
        &self,
        resolver: &mut FileIconResolver,
        stats: &mut IconRolePrewarmStats,
        deadline: Instant,
        limit: usize,
    ) {
        for _ in 0..limit {
            if Instant::now() >= deadline {
                stats.over_budget = true;
                return;
            }
            let Some(request) = self.icon_role_read_ahead.borrow_mut().pop_front() else {
                return;
            };
            let resolve_start = Instant::now();
            let snapshot = resolver.resolve_path_cache_key(request.key);
            stats.resolve_us += resolve_start.elapsed().as_micros();
            stats.read_ahead += 1;
            if snapshot.is_none() {
                stats.deferred += 1;
            }
            let _ = snapshot;
        }
    }

    fn prewarm_file_item_text_labels(
        &self,
        projections: &[ShellPaneProjection<'_>],
        text: &mut TextFrameBuilder<'_>,
        mode: TextLabelPrewarmMode,
    ) -> TextLabelPrewarmStats {
        let mut stats = TextLabelPrewarmStats::default();
        let raster_us_start = text.raster_us;
        let deadline = Instant::now() + text_label_prewarm_budget_for_mode(mode);
        let theme = self.theme();

        for projection in projections {
            for item in &projection.visible_items {
                if Instant::now() >= deadline {
                    stats.over_budget = true;
                    stats.raster_us = text.raster_us.saturating_sub(raster_us_start);
                    return stats;
                }
                let outcome =
                    self.prewarm_projection_text_label(projection, item.layout, text, theme);
                if outcome != LabelCacheOutcome::Skipped {
                    stats.entries += 1;
                }
                stats.record(outcome);
            }
        }

        stats.raster_us = text.raster_us.saturating_sub(raster_us_start);
        stats
    }

    fn prewarm_projection_text_label(
        &self,
        projection: &ShellPaneProjection<'_>,
        layout: ItemLayout,
        text: &mut TextFrameBuilder<'_>,
        theme: ShellTheme,
    ) -> LabelCacheOutcome {
        let Some(entry_index) = projection
            .view
            .filtered_indexes
            .get(layout.model_index)
            .copied()
        else {
            return LabelCacheOutcome::Skipped;
        };
        let Some(entry) = projection.view.entries.get(entry_index) else {
            return LabelCacheOutcome::Skipped;
        };
        let selected = projection.view.selection.contains(entry_index);
        let text_color = pane_item_text_color(projection.view.view_mode, entry, selected, theme);
        match projection.view.view_mode {
            ShellViewMode::Compact => text.prewarm_label_aligned_wrapped(
                entry.name.as_ref(),
                layout.text_rect,
                text_color,
                LabelAlignment::Start,
                LabelWrap::None,
            ),
            ShellViewMode::Details => text.prewarm_filename_label_aligned_no_wrap(
                entry.name.as_ref(),
                layout.text_rect,
                text_color,
                LabelAlignment::Start,
            ),
            ShellViewMode::Icons => text.prewarm_filename_label_wrapped(
                entry.name.as_ref(),
                layout.text_rect,
                text_color,
            ),
        }
    }

    fn push_pane_projection(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        projection: &ShellPaneProjection<'_>,
        size: PhysicalSize<u32>,
        paint: ShellPaintPalettes,
    ) -> bool {
        let theme = paint.shell;
        let pane_id = projection.geometry.kind;
        let pane = projection.geometry.pane;
        let top_bar = projection.geometry.top_bar;
        let status_bar = projection.geometry.status_bar;
        let screen = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        let pane_radius = self.scale_metric(10.0);
        push_clipped_rounded_rect(vertices, pane, screen, pane_radius, theme.divider(), size);
        if let Some(inner) = inset_rect(pane, self.scale_metric(1.0)) {
            push_clipped_rounded_rect(
                vertices,
                inner,
                screen,
                (pane_radius - self.scale_metric(1.0)).max(1.0),
                theme.view_mode_content(projection.view.view_mode),
                size,
            );
        }

        push_rect(vertices, top_bar, theme.chrome(), size);
        if let Some(path_rect) = self.pane_path_bar_rect(pane_id, size) {
            let location_active = self.location_bar_active_for_pane(pane_id);
            let path_label = self.location_label_for_pane(pane_id);
            let path_cursor = self.location_cursor_for_pane(pane_id);
            self.push_location_bar(
                vertices,
                text,
                size,
                path_rect,
                top_bar,
                &path_label,
                location_active,
                path_cursor,
                theme,
            );
        }

        push_rect(
            vertices,
            ViewRect {
                x: pane.x,
                y: top_bar.bottom(),
                width: pane.width,
                height: (status_bar.y - top_bar.bottom()).max(1.0),
            },
            theme.view_mode_content(projection.view.view_mode),
            size,
        );
        self.push_pane_body_border(vertices, projection, theme, size);
        if pane_id == ShellPaneId::SLOT_0 {
            self.push_filter_bar(vertices, text, size, theme);
        }
        if projection.view.view_mode == ShellViewMode::Details {
            self.push_details_header_for_projection(vertices, text, projection, size, theme);
        }

        let item_palette = paint.dolphin_item;
        self.push_path_transition_exit_projection(vertices, text, icons, projection, size, paint);
        let enter_process = shell::path_transition::enter_process_for_pane(self, pane_id);
        for item in projection.visible_items.iter().copied() {
            self.push_pane_item_with_transition(
                vertices,
                text,
                icons,
                projection,
                item,
                item_palette,
                size,
                theme,
                ShellPaneItemTransitionPaint::Enter {
                    process: enter_process,
                },
            );
        }
        if self.rubber_band.is_some() && pane_id == self.active_pane() {
            self.push_rubber_band_for_projection(vertices, projection, theme, size);
        }
        let content_scrollbar_visible =
            self.push_content_scrollbar_for_projection(vertices, projection, theme, size);
        self.push_pane_status_bar(vertices, text, projection, size, theme);
        push_clipped_rect_outline(
            vertices,
            pane,
            screen,
            self.scale_metric(1.0).max(1.0),
            theme.divider(),
            size,
        );
        content_scrollbar_visible
    }

    fn push_path_transition_exit_projection(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        projection: &ShellPaneProjection<'_>,
        size: PhysicalSize<u32>,
        paint: ShellPaintPalettes,
    ) {
        let pane = projection.geometry.kind;
        let Some(exit_process) = shell::path_transition::exit_process_for_pane(self, pane) else {
            return;
        };
        let alpha = shell::path_transition::opacity_for_process(exit_process);
        if alpha <= 0.01 {
            return;
        }
        let Some(snapshot) = shell::path_transition::exit_snapshot_for_pane(self, pane) else {
            return;
        };
        let old_view = ShellPaneView::from_state(&snapshot.state);
        let old_projection = ShellPaneProjection {
            view: old_view,
            geometry: snapshot.geometry,
            visible_items: snapshot.visible_items.clone(),
            scroll_metrics: snapshot.scroll_metrics,
        };
        let theme = paint.shell;
        let item_palette = paint.dolphin_item;
        let mut background = theme.view_mode_content(old_projection.view.view_mode);
        background[3] *= alpha;
        push_clipped_rect(
            vertices,
            shell::path_transition::transform_rect_for_process(
                old_projection.geometry.content,
                old_projection.geometry.content,
                exit_process,
            ),
            old_projection.geometry.content,
            background,
            size,
        );
        for item in old_projection.visible_items.iter().copied() {
            self.push_pane_item_with_transition(
                vertices,
                text,
                icons,
                &old_projection,
                item,
                item_palette,
                size,
                theme,
                ShellPaneItemTransitionPaint::Exit {
                    process: exit_process,
                },
            );
        }
    }

    fn push_pane_item_with_transition(
        &self,
        vertices: &mut Vec<QuadVertex>,
        text: &mut TextFrameBuilder<'_>,
        icons: &mut IconFrameBuilder<'_>,
        projection: &ShellPaneProjection<'_>,
        item: ShellPaneVisibleItem,
        item_palette: DolphinItemPalette,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
        transition: ShellPaneItemTransitionPaint,
    ) {
        let layout = item.layout;
        let _slot_id = item.slot_id;
        let Some(entry_index) = projection
            .view
            .filtered_indexes
            .get(layout.model_index)
            .copied()
        else {
            return;
        };
        let Some(entry) = projection.view.entries.get(entry_index) else {
            return;
        };
        let entry_path = self.entry_path_for_pane_view(projection.view, entry_index);
        let (reflow_dx, reflow_dy) = if transition.entering() {
            entry_path
                .as_deref()
                .and_then(|path| self.item_reflow_offset_for_path(projection.geometry.kind, path))
                .unwrap_or((0.0, 0.0))
        } else {
            (0.0, 0.0)
        };
        let alpha = transition.alpha();
        let vertex_start = vertices.len();
        let previous_icon_alpha = icons.replace_content_alpha(alpha);
        let content_clip = projection.geometry.content;
        let item_rect = translated_rect(
            pane_content_rect_to_screen(layout.item_rect, projection),
            reflow_dx,
            reflow_dy,
        );
        let visual_rect = translated_rect(
            pane_content_rect_to_screen(layout.visual_rect, projection),
            reflow_dx,
            reflow_dy,
        );
        let icon_rect = translated_rect(
            pane_content_rect_to_screen(layout.icon_rect, projection),
            reflow_dx,
            reflow_dy,
        );
        let text_rect = translated_rect(
            pane_content_rect_to_screen(layout.text_rect, projection),
            reflow_dx,
            reflow_dy,
        );
        let untransformed_item_rect = item_rect;
        let untransformed_text_rect = text_rect;
        let item_rect = transition.transform_rect(item_rect, content_clip);
        let visual_rect = transition.transform_rect(visual_rect, content_clip);
        let icon_rect = transition.transform_rect(icon_rect, content_clip);
        let text_rect = transition.transform_rect(text_rect, content_clip);
        let content_rect = visual_rect;
        let pixmap_layout = ItemPixmapLayout {
            view_mode: projection.view.view_mode,
            icon_rect,
            text_rect,
            text_midline_shift: text.dolphin_midline_shift(),
        };
        let selected = projection.view.selection.contains(entry_index);
        let hovered = transition.entering()
            && self.hovered_item
                == Some(ShellPaneItemTarget {
                    pane: projection.geometry.kind,
                    index: entry_index,
                });
        let dnd_hovered = transition.entering()
            && matches!(
                self.dnd_hover_target,
                Some(ShellDropTarget::PaneItem {
                    pane,
                    index,
                    is_dir: true,
                    ..
                }) if pane == projection.geometry.kind && index == entry_index
            );
        let current = transition.entering()
            && projection.geometry.kind == self.active_pane()
            && projection.view.selection.focus == Some(entry_index);
        let hover_progress = if hovered {
            self.hover_animation_factor()
        } else {
            1.0
        };
        let paint = dolphin_item_paint_with_palette_and_hover_progress(
            projection.view.view_mode,
            item_rect,
            visual_rect,
            content_rect,
            selected,
            hovered,
            current,
            entry_index % 2 == 1,
            self.ui_scale(),
            item_palette,
            hover_progress,
        );

        if let Some(background) = paint.alternate_background {
            push_clipped_rect(
                vertices,
                background.rect,
                content_clip,
                background.color,
                size,
            );
        }
        if let Some(background) = paint.background {
            if background.radius <= 0.0 {
                push_clipped_rect(
                    vertices,
                    background.rect,
                    content_clip,
                    background.color,
                    size,
                );
            } else {
                push_clipped_rounded_rect(
                    vertices,
                    background.rect,
                    content_clip,
                    background.radius,
                    background.color,
                    size,
                );
            }
        }
        if let Some(focus) = paint.focus {
            push_clipped_rounded_highlight(
                vertices,
                focus.rect,
                content_clip,
                focus.radius,
                [0.0, 0.0, 0.0, 0.0],
                focus.color,
                focus.stroke_width,
                size,
            );
        }
        if dnd_hovered {
            let radius = self.scale_metric(7.0);
            let drop_target = theme.drop_target();
            push_clipped_rounded_highlight(
                vertices,
                content_rect,
                content_clip,
                radius,
                drop_target.fill,
                drop_target.border,
                self.scale_metric(1.0),
                size,
            );
        }

        let folder_preview =
            self.folder_preview_role_for_pane_entry(projection.view, entry_index, pixmap_layout);
        if !icons.push_thumbnail_or_icon(
            projection.view.path,
            entry,
            folder_preview.as_ref(),
            pixmap_layout,
            content_clip,
        ) {
            push_fallback_file_icon(vertices, entry, icon_rect, content_clip, theme, size);
        }

        let base_text_color =
            pane_item_text_color(projection.view.view_mode, entry, selected, theme);
        let text_color = text_color_with_alpha_factor(base_text_color, alpha);
        let muted_text = text_color_with_alpha_factor(theme.muted_text(), alpha);
        match projection.view.view_mode {
            ShellViewMode::Compact => {
                text.push_label_aligned_wrapped_with_layout(
                    entry.name.as_ref(),
                    text_rect,
                    untransformed_text_rect,
                    content_clip,
                    text_color,
                    LabelAlignment::Start,
                    LabelWrap::None,
                );
            }
            ShellViewMode::Details => {
                text.push_filename_label_aligned_no_wrap_with_layout(
                    entry.name.as_ref(),
                    text_rect,
                    untransformed_text_rect,
                    content_clip,
                    text_color,
                    LabelAlignment::Start,
                );
            }
            ShellViewMode::Icons => {
                text.push_filename_label_wrapped_with_layout(
                    entry.name.as_ref(),
                    text_rect,
                    untransformed_text_rect,
                    content_clip,
                    text_color,
                );
            }
        }

        if projection.view.view_mode == ShellViewMode::Details {
            let text_height = self.text_line_height();
            let metadata_y = untransformed_item_rect.y
                + (untransformed_item_rect.height - text_height).max(0.0) / 2.0;
            let size_rect = transition.transform_rect(
                ViewRect {
                    x: content_clip.x + self.details_name_width() + self.scale_metric(8.0)
                        - projection.view.scroll_x
                        + reflow_dx,
                    y: metadata_y,
                    width: self.details_size_width() - self.scale_metric(16.0),
                    height: text_height,
                },
                content_clip,
            );
            text.push_label_aligned_no_wrap(
                &details_size_label(entry),
                size_rect,
                content_clip,
                muted_text,
                LabelAlignment::Start,
            );
            let modified_rect = transition.transform_rect(
                ViewRect {
                    x: content_clip.x
                        + self.details_name_width()
                        + self.details_size_width()
                        + self.scale_metric(8.0)
                        - projection.view.scroll_x
                        + reflow_dx,
                    y: metadata_y,
                    width: self.details_modified_width() - self.scale_metric(16.0),
                    height: text_height,
                },
                content_clip,
            );
            text.push_label_aligned_no_wrap(
                &format_modified_secs(entry.modified_secs),
                modified_rect,
                content_clip,
                muted_text,
                LabelAlignment::Start,
            );
        }
        icons.replace_content_alpha(previous_icon_alpha);
        fade_quad_vertices_alpha(&mut vertices[vertex_start..], alpha);
    }
}
