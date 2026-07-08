
    fn raster_pixel(raster: &IconRaster, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * raster.width + x) * 4) as usize;
        [
            raster.pixels[offset],
            raster.pixels[offset + 1],
            raster.pixels[offset + 2],
            raster.pixels[offset + 3],
        ]
    }

    fn raster_has_visible_pixel_in_rect(
        raster: &IconRaster,
        rect: FolderPreviewThumbnailSlot,
    ) -> bool {
        let right = rect.x.saturating_add(rect.width).min(raster.width);
        let bottom = rect.y.saturating_add(rect.height).min(raster.height);
        for y in rect.y.min(raster.height)..bottom {
            for x in rect.x.min(raster.width)..right {
                if raster_pixel(raster, x, y)[3] > 0 {
                    return true;
                }
            }
        }
        false
    }

    fn raster_has_visible_pixel_outside_slots(
        raster: &IconRaster,
        slots: &[FolderPreviewThumbnailSlot],
    ) -> bool {
        for y in 0..raster.height {
            for x in 0..raster.width {
                if raster_pixel(raster, x, y)[3] == 0 {
                    continue;
                }
                let inside_slot = slots.iter().any(|slot| {
                    x >= slot.x
                        && y >= slot.y
                        && x < slot.x.saturating_add(slot.width)
                        && y < slot.y.saturating_add(slot.height)
                });
                if !inside_slot {
                    return true;
                }
            }
        }
        false
    }

    fn wait_for_thumbnail_state(
        resolver: &mut ThumbnailRasterResolver,
        path: &Path,
        modified_secs: u64,
        mime_type: Option<&str>,
        size_px: u16,
    ) -> ThumbnailResolveState {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let state =
                resolver.resolve(path, modified_secs, mime_type.map(str::to_string), size_px);
            if !matches!(state, ThumbnailResolveState::Pending) || Instant::now() >= deadline {
                return state;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn write_test_thumbnail_png(path: &Path, uri: &str, modified_secs: u64) {
        write_test_thumbnail_png_with_color(path, uri, modified_secs, [32, 96, 192, 255]);
    }

    fn write_test_thumbnail_png_with_color(
        path: &Path,
        uri: &str,
        modified_secs: u64,
        color: [u8; 4],
    ) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbaImage::from_pixel(4, 4, image::Rgba(color))
            .save(path)
            .unwrap();
        fika_core::write_thumbnail_metadata(path, uri, modified_secs).unwrap();
    }

    fn raster_contains_rgb(raster: &IconRaster, expected: [u8; 3]) -> bool {
        raster.pixels.chunks_exact(4).any(|pixel| {
            pixel[3] > 200
                && pixel[..3]
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| actual.abs_diff(expected) <= 12)
        })
    }

    fn raster_contains_rgb_in_rect(
        raster: &IconRaster,
        expected: [u8; 3],
        rect: FolderPreviewThumbnailSlot,
    ) -> bool {
        let right = rect.x.saturating_add(rect.width).min(raster.width);
        let bottom = rect.y.saturating_add(rect.height).min(raster.height);
        for y in rect.y.min(raster.height)..bottom {
            for x in rect.x.min(raster.width)..right {
                let pixel = raster_pixel(raster, x, y);
                if pixel[3] > 200
                    && pixel[..3]
                        .iter()
                        .zip(expected)
                        .all(|(actual, expected)| actual.abs_diff(expected) <= 12)
                {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn split_pane_mouse_wheel_scrolls_the_target_pane_only() {
        let mut scene = test_scene(
            (0..12)
                .map(|index| test_entry(&format!("left-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let split_entries = (0..80)
            .map(|index| test_entry(&format!("right-{index:02}.txt"), false))
            .collect::<Vec<_>>();
        scene.panes.set(
            ShellPaneId::SLOT_1,
            ShellPaneState {
                path: PathBuf::from("/right-root"),
                view_mode: ShellViewMode::Icons,
                zoom_step: 0,
                dir_count: 0,
                filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
                entries: split_entries,
                selection: ShellSelection::default(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        );
        let size = PhysicalSize::new(760, 260);
        let split_content = scene
            .pane_geometry(ShellPaneId::SLOT_1, size)
            .unwrap()
            .content;
        scene.pointer = Some(ViewPoint {
            x: split_content.x + 8.0,
            y: split_content.y + 8.0,
        });

        assert!(scene.scroll_by(120.0, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].scroll_y, 0.0);
        assert!(scene.panes.get(ShellPaneId::SLOT_1).unwrap().scroll_y > 0.0);
        assert_eq!(scene.content_scroll_changes, 1);
    }

    #[test]
    fn split_pane_scrollbar_drag_updates_split_scroll_offset() {
        let mut scene = test_scene(
            (0..12)
                .map(|index| test_entry(&format!("left-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let split_entries = (0..80)
            .map(|index| test_entry(&format!("right-{index:02}.txt"), false))
            .collect::<Vec<_>>();
        scene.panes.set(
            ShellPaneId::SLOT_1,
            ShellPaneState {
                path: PathBuf::from("/right-root"),
                view_mode: ShellViewMode::Icons,
                zoom_step: 0,
                dir_count: 0,
                filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
                entries: split_entries,
                selection: ShellSelection::default(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        );
        let size = PhysicalSize::new(760, 260);
        let (track, thumb) = scene
            .pane_content_scrollbar_rects(ShellPaneId::SLOT_1, size)
            .expect("split pane should need its own scrollbar");
        let press = ViewPoint {
            x: thumb.x + thumb.width / 2.0,
            y: thumb.y + thumb.height / 2.0,
        };
        let drag_to = ViewPoint {
            x: press.x,
            y: track.bottom() - thumb.height / 2.0,
        };

        assert!(scene.begin_scrollbar_drag(press, size).is_some());
        assert_eq!(
            scene.scrollbar_drag.map(|drag| drag.target),
            Some(ScrollbarDragTarget::Content {
                pane: ShellPaneId::SLOT_1,
                axis: ContentScrollbarAxis::Vertical,
            })
        );
        assert!(scene.set_pointer(drag_to, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].scroll_y, 0.0);
        assert!(scene.panes.get(ShellPaneId::SLOT_1).unwrap().scroll_y > 0.0);
        assert_eq!(scene.content_scroll_changes, 1);
        let _ = scene.end_scrollbar_drag(drag_to, size);
        assert!(scene.scrollbar_drag.is_none());
    }

    #[test]
    fn places_sidebar_scroll_is_independent_from_file_content_scroll() {
        let entries = (0..80)
            .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
            .collect::<Vec<_>>();
        let mut scene = test_scene(entries, ShellViewMode::Icons);
        scene.places = (0..28)
            .map(|index| {
                ShellPlace::new(
                    "",
                    "B",
                    format!("Place {index:02}"),
                    PathBuf::from(format!("/tmp/place-{index:02}")),
                    true,
                )
            })
            .collect();
        let size = PhysicalSize::new(700, 220);
        assert!(scene.max_places_scroll_y(size) > 0.0);
        assert!(scene.max_scroll_y(size) > 0.0);

        scene.pointer = Some(ViewPoint {
            x: PLACES_SIDEBAR_PADDING_X + 2.0,
            y: TOP_BAR_HEIGHT + 10.0,
        });
        assert!(scene.scroll_by(90.0, size));
        assert!(scene.places_scroll_y > 0.0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].scroll_y, 0.0);
        assert_eq!(scene.places_scroll_changes, 1);
        assert_eq!(scene.content_scroll_changes, 0);

        scene.pointer = Some(ViewPoint {
            x: scene.content_origin_x(size) + 10.0,
            y: scene.content_origin_y() + 10.0,
        });
        assert!(scene.scroll_by(90.0, size));
        assert!(scene.panes[ShellPaneId::SLOT_0].scroll_y > 0.0);
        assert_eq!(scene.places_scroll_changes, 1);
        assert_eq!(scene.content_scroll_changes, 1);
    }

    #[test]
    fn places_row_hit_testing_follows_sidebar_scroll_offset() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = (0..16)
            .map(|index| {
                ShellPlace::new(
                    "",
                    "B",
                    format!("Place {index:02}"),
                    PathBuf::from(format!("/tmp/place-{index:02}")),
                    true,
                )
            })
            .collect();
        let size = PhysicalSize::new(700, 160);
        let first_row = scene.place_row_rects(size)[0].1;
        let point = ViewPoint {
            x: first_row.x + 6.0,
            y: first_row.y + 6.0,
        };

        assert_eq!(scene.place_index_at_screen_point(point, size), Some(0));
        assert!(scene.scroll_places_by(PLACES_ROW_HEIGHT + PLACES_ROW_GAP, size));
        assert_eq!(scene.place_index_at_screen_point(point, size), Some(1));
    }

    #[test]
    fn places_scrollbar_thumb_moves_with_sidebar_scroll() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = (0..24)
            .map(|index| {
                ShellPlace::new(
                    "",
                    "B",
                    format!("Place {index:02}"),
                    PathBuf::from(format!("/tmp/place-{index:02}")),
                    true,
                )
            })
            .collect();
        let size = PhysicalSize::new(700, 220);
        let before = scene
            .places_scrollbar_thumb_rect(size)
            .expect("overflowing places should show a scrollbar thumb");

        assert!(scene.scroll_places_by(96.0, size));
        let after = scene
            .places_scrollbar_thumb_rect(size)
            .expect("scrollbar thumb should remain visible");

        assert!(after.y > before.y);
        assert_eq!(after.x, before.x);
        assert_eq!(after.width, before.width);
    }

    #[test]
    fn content_scrollbar_reserves_vertical_track_for_icons() {
        let scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);
        let content = scene
            .pane_geometry(ShellPaneId::SLOT_0, size)
            .unwrap()
            .content;
        let (track, thumb) = scene
            .content_scrollbar_rects(size)
            .expect("icons view should need vertical scrollbar");

        assert_eq!(
            scene.content_scrollbar_axis(),
            ContentScrollbarAxis::Vertical
        );
        assert!(track.x >= content.right());
        assert!(track.width > 0.0);
        assert!(thumb.height >= CONTENT_SCROLLBAR_MIN_THUMB_SIZE.min(track.height));
        assert!(!content.contains(ViewPoint {
            x: track.x,
            y: track.y,
        }));
    }

    #[test]
    fn compact_content_scrollbar_uses_horizontal_offset() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Compact,
        );
        let size = PhysicalSize::new(420, 260);
        let (start_track, start_thumb) = scene
            .content_scrollbar_rects(size)
            .expect("compact view should need horizontal scrollbar");
        scene.panes[ShellPaneId::SLOT_0].scroll_x = scene.max_scroll_x(size) / 2.0;
        let (middle_track, middle_thumb) = scene
            .content_scrollbar_rects(size)
            .expect("compact view should keep horizontal scrollbar");

        assert_eq!(
            scene.content_scrollbar_axis(),
            ContentScrollbarAxis::Horizontal
        );
        assert_eq!(start_track.y, middle_track.y);
        assert!(middle_thumb.x > start_thumb.x);
        assert_eq!(start_thumb.height, middle_thumb.height);
    }

    #[test]
    fn content_scrollbar_thumb_drag_updates_vertical_scroll() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);
        let (track, thumb) = scene
            .content_scrollbar_rects(size)
            .expect("icons view should need vertical scrollbar");
        let press = ViewPoint {
            x: thumb.x + thumb.width / 2.0,
            y: thumb.y + thumb.height / 2.0,
        };
        let drag_to = ViewPoint {
            x: press.x,
            y: track.bottom() - thumb.height / 2.0,
        };

        assert!(scene.begin_scrollbar_drag(press, size).is_some());
        assert!(scene.scrollbar_drag.is_some());
        assert!(scene.set_pointer(drag_to, size));
        assert!(scene.panes[ShellPaneId::SLOT_0].scroll_y > 0.0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].scroll_x, 0.0);
        assert_eq!(scene.content_scroll_changes, 1);
        let _ = scene.end_scrollbar_drag(drag_to, size);
        assert!(scene.scrollbar_drag.is_none());
    }

    #[test]
    fn content_scrollbar_thumb_drag_updates_horizontal_scroll() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Compact,
        );
        let size = PhysicalSize::new(420, 260);
        let (track, thumb) = scene
            .content_scrollbar_rects(size)
            .expect("compact view should need horizontal scrollbar");
        let press = ViewPoint {
            x: thumb.x + thumb.width / 2.0,
            y: thumb.y + thumb.height / 2.0,
        };
        let drag_to = ViewPoint {
            x: track.right() - thumb.width / 2.0,
            y: press.y,
        };

        assert!(scene.begin_scrollbar_drag(press, size).is_some());
        assert!(scene.set_pointer(drag_to, size));
        assert!(scene.panes[ShellPaneId::SLOT_0].scroll_x > 0.0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].scroll_y, 0.0);
        let _ = scene.end_scrollbar_drag(drag_to, size);
        assert!(scene.scrollbar_drag.is_none());
    }

    #[test]
    fn places_scrollbar_thumb_drag_updates_sidebar_scroll() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = (0..24)
            .map(|index| {
                ShellPlace::new(
                    "",
                    "B",
                    format!("Place {index:02}"),
                    PathBuf::from(format!("/tmp/place-{index:02}")),
                    true,
                )
            })
            .collect();
        let size = PhysicalSize::new(700, 220);
        let (track, thumb) = scene
            .places_scrollbar_rects(size)
            .expect("overflowing places should show a scrollbar");
        let press = ViewPoint {
            x: thumb.x + thumb.width / 2.0,
            y: thumb.y + thumb.height / 2.0,
        };
        let drag_to = ViewPoint {
            x: press.x,
            y: track.bottom() - thumb.height / 2.0,
        };

        assert!(scene.begin_scrollbar_drag(press, size).is_some());
        assert!(scene.set_pointer(drag_to, size));
        assert!(scene.places_scroll_y > 0.0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].scroll_y, 0.0);
        assert!(scene.places_scroll_changes > 0);
        let _ = scene.end_scrollbar_drag(drag_to, size);
        assert!(scene.scrollbar_drag.is_none());
    }
