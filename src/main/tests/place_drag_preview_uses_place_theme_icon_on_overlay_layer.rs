
    #[test]
    fn place_drag_preview_uses_place_theme_icon_on_overlay_layer() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![ShellPlace::new(
            "",
            "H",
            "Home",
            PathBuf::from("/tmp"),
            false,
        )];
        let icon_name = scene.places[0].icon_name;
        let icon_size = icon_cache_size(scene.scale_metric(128.0));
        let icon_path = PathBuf::from("/tmp/fika-place-preview-icon.png");
        let key = FileIconPathCacheKey {
            role: FileIconRoleCacheKey {
                kind: FileIconKind::Named {
                    icon_name: icon_name.to_string(),
                    fallback: NamedIconFallback::Service,
                },
            },
            size_px: icon_size,
        };
        let size = PhysicalSize::new(700, 360);
        let start = ViewPoint { x: 120.0, y: 120.0 };
        assert!(scene.begin_internal_drag_for_place(0, start));
        assert!(scene.set_pointer(ViewPoint { x: 136.0, y: 136.0 }, size));

        let mut vertices = Vec::new();
        let mut font_system = FontSystem::new();
        let mut swash_cache = SwashCache::new();
        let mut text_buffer = Buffer::new_empty(Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT));
        let mut label_cache = LabelRasterCache::new(1024 * 1024);
        let mut metrics_cache = LabelMetricsCache::new(TEXT_LABEL_METRICS_CACHE_MAX_ENTRIES);
        let mut atlas_cache = TextAtlasFrameCache::default();
        let mut icon_harness = FileIconResolverTestHarness::new();
        icon_harness.complete(key, Some(icon_path.clone()));
        let mut thumbnails = ThumbnailRasterResolver::new();
        let mut icon_rasters = IconRasterResolver::new();
        let mut raster_cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
        raster_cache.insert(
            IconRasterCacheKey::icon(icon_path, icon_size),
            test_icon_raster(8, 4),
        );
        let mut role_raster_cache = IconRoleRasterCache::new(ICON_ROLE_RASTER_CACHE_MAX_BYTES);
        let mut text = TextFrameBuilder::new(
            &mut font_system,
            &mut swash_cache,
            &mut text_buffer,
            &mut label_cache,
            &mut metrics_cache,
            &mut atlas_cache,
            size,
            scene.ui_scale(),
            Vec::new(),
        );
        let mut icons = IconFrameBuilder::new(
            &mut icon_harness.resolver,
            &mut thumbnails,
            &mut icon_rasters,
            &mut raster_cache,
            &mut role_raster_cache,
            size,
            0,
            0,
            0,
        );
        scene.push_drag_preview_overlay(&mut vertices, &mut text, &mut icons, scene.theme(), size);
        let frame = icons.finish();

        assert!(!frame.overlay_vertices.is_empty());
        assert!(frame.vertices.is_empty());
    }

    #[test]
    fn split_panes_share_reusable_state_accessors() {
        let mut scene = test_scene(vec![test_entry("alpha", true)], ShellViewMode::Icons);
        let split_entries = vec![test_entry("right", true)];
        scene.panes.set(
            ShellPaneId::SLOT_1,
            ShellPaneState {
                path: PathBuf::from("/right-root"),
                view_mode: ShellViewMode::Details,
                zoom_step: 0,
                dir_count: 1,
                filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
                entries: split_entries,
                selection: ShellSelection::default(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        );

        assert_eq!(
            scene.pane_state(ShellPaneId::SLOT_0).unwrap().path,
            PathBuf::from("/tmp")
        );
        assert_eq!(
            scene.pane_state(ShellPaneId::SLOT_1).unwrap().path,
            PathBuf::from("/right-root")
        );

        scene.pane_state_mut(ShellPaneId::SLOT_0).unwrap().scroll_y = 42.0;
        scene.pane_state_mut(ShellPaneId::SLOT_1).unwrap().scroll_y = 24.0;

        assert_eq!(
            scene.pane_scroll_offset(ShellPaneId::SLOT_0),
            Some((0.0, 42.0))
        );
        assert_eq!(
            scene.pane_scroll_offset(ShellPaneId::SLOT_1),
            Some((0.0, 24.0))
        );
    }

    #[test]
    fn split_pane_click_updates_active_pane_for_reused_routing() {
        let mut scene = test_scene(vec![test_entry("alpha", true)], ShellViewMode::Icons);
        let split_entries = vec![test_entry("right", true)];
        scene.panes.set(
            ShellPaneId::SLOT_1,
            ShellPaneState {
                path: PathBuf::from("/right-root"),
                view_mode: ShellViewMode::Icons,
                zoom_step: 0,
                dir_count: 1,
                filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
                entries: split_entries,
                selection: ShellSelection::default(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        );
        let size = PhysicalSize::new(900, 360);
        let split_content = scene
            .pane_geometry(ShellPaneId::SLOT_1, size)
            .unwrap()
            .content;
        let point = ViewPoint {
            x: split_content.x + 6.0,
            y: split_content.y + 6.0,
        };

        assert_eq!(scene.active_pane(), ShellPaneId::SLOT_0);
        assert!(scene.focus_pane_at_screen_point(point, size));
        assert_eq!(scene.active_pane(), ShellPaneId::SLOT_1);
        assert!(!scene.focus_pane_at_screen_point(point, size));
    }

    #[test]
    fn split_pane_hover_tracks_pane_item_target() {
        let mut scene = test_scene(vec![test_entry("alpha", true)], ShellViewMode::Icons);
        let split_entries = vec![test_entry("right", true), test_entry("other", false)];
        scene.panes.set(
            ShellPaneId::SLOT_1,
            ShellPaneState {
                path: PathBuf::from("/right-root"),
                view_mode: ShellViewMode::Icons,
                zoom_step: 0,
                dir_count: 1,
                filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
                entries: split_entries,
                selection: ShellSelection::default(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        );
        let size = PhysicalSize::new(900, 360);
        let projection = scene.pane_projection(ShellPaneId::SLOT_1, size).unwrap();
        let item = projection.visible_items[0].layout.visual_rect;
        let point = ViewPoint {
            x: projection.geometry.content.x + item.x + 4.0,
            y: projection.geometry.content.y + item.y + 4.0,
        };

        assert!(scene.set_pointer(point, size));
        assert_eq!(
            scene.hovered_item,
            Some(ShellPaneItemTarget {
                pane: ShellPaneId::SLOT_1,
                index: 0,
            })
        );
    }

    #[test]
    fn split_pane_file_context_split_path_falls_back_to_that_pane() {
        let mut scene = test_scene(vec![test_entry("left.txt", false)], ShellViewMode::Icons);
        set_test_pane(
            &mut scene,
            ShellPaneId::SLOT_1,
            PathBuf::from("/right-root"),
            ShellViewMode::Icons,
            vec![test_entry("right.txt", false)],
        );
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_1,
            index: 0,
            path: PathBuf::from("right.txt"),
            is_dir: false,
            selection_count: 1,
        });

        assert_eq!(
            scene.context_target_split_pane_path(),
            Some(PathBuf::from("/right-root"))
        );
    }

    #[test]
    fn split_pane_path_bar_click_activates_location_editor_for_that_pane() {
        let mut scene = test_scene(vec![test_entry("alpha", true)], ShellViewMode::Icons);
        scene.panes.set(
            ShellPaneId::SLOT_1,
            ShellPaneState {
                path: PathBuf::from("/right-root"),
                view_mode: ShellViewMode::Icons,
                zoom_step: 0,
                dir_count: 0,
                filtered_indexes: Vec::new(),
                entries: Vec::new(),
                selection: ShellSelection::default(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        );
        let size = PhysicalSize::new(900, 360);
        let rect = scene
            .pane_path_bar_rect(ShellPaneId::SLOT_1, size)
            .expect("split path bar should be visible");
        let point = ViewPoint {
            x: rect.x + 8.0,
            y: rect.y + rect.height / 2.0,
        };

        assert!(scene.activate_path_bar_at_screen_point(point, size));
        assert_eq!(scene.active_pane(), ShellPaneId::SLOT_1);
        assert!(scene.is_location_editing());
        assert_eq!(scene.location_draft_pane(), Some(ShellPaneId::SLOT_1));
        assert_eq!(scene.location_draft_value(), Some("/right-root"));
        assert!(scene.location_bar_active_for_pane(ShellPaneId::SLOT_1));
        assert!(!scene.location_bar_active_for_pane(ShellPaneId::SLOT_0));
    }

    #[test]
    fn places_open_into_active_split_pane() {
        let root = test_dir("places-active-split");
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("child.txt"), "split").unwrap();

        let mut scene = test_scene(vec![test_entry("left", true)], ShellViewMode::Icons);
        scene.panes[ShellPaneId::SLOT_0].path = root.clone();
        scene.panes.set(
            ShellPaneId::SLOT_1,
            ShellPaneState {
                path: root.join("old-split"),
                view_mode: ShellViewMode::Icons,
                zoom_step: 0,
                dir_count: 0,
                filtered_indexes: Vec::new(),
                entries: Vec::new(),
                selection: ShellSelection::default(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        );
        scene.active_pane = ShellPaneId::SLOT_1;
        scene.places = vec![ShellPlace::new("", "T", "Target", target.clone(), true)];
        let size = PhysicalSize::new(900, 360);
        let row = scene.place_row_rects(size)[0].1;
        let activation = scene
            .place_activation_for_press(
                ViewPoint {
                    x: row.x + 4.0,
                    y: row.y + 4.0,
                },
                size,
            )
            .expect("place should activate");
        let ShellPlaceActivation::Open { pane, path } = activation else {
            panic!("place should open a path");
        };

        assert_eq!(pane, ShellPaneId::SLOT_1);
        assert_eq!(path, target);
        assert!(scene.load_path_for_pane(pane, path, size).unwrap());
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, root);
        let split = scene.panes.get(ShellPaneId::SLOT_1).unwrap();
        assert_eq!(split.path, target);
        assert_eq!(split.entries.len(), 1);
        assert_eq!(split.entries[0].name.as_ref(), "child.txt");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn split_pane_location_commit_and_history_are_pane_local() {
        let root = test_dir("split-location-history");
        let left = root.join("left");
        let right = root.join("right");
        let next = root.join("next");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::create_dir_all(&next).unwrap();
        fs::write(left.join("left.txt"), b"left").unwrap();
        fs::write(right.join("right.txt"), b"right").unwrap();
        fs::write(next.join("next.txt"), b"next").unwrap();

        let size = PhysicalSize::new(900, 360);
        let mut scene = ShellScene::load(left.clone(), ShellViewMode::Icons).unwrap();
        assert!(scene.open_split_pane(right.clone(), size).unwrap());
        scene.active_pane = ShellPaneId::SLOT_1;

        assert!(scene.apply_location_command(LocationCommand::Activate, size));
        scene
            .location_draft
            .as_mut()
            .unwrap()
            .draft
            .set_completed(next.display().to_string());
        assert_eq!(
            scene.resolved_location_draft(),
            Some((ShellPaneId::SLOT_1, next.clone()))
        );
        assert!(scene.close_location_draft(size));
        assert!(
            scene
                .load_path_for_pane(ShellPaneId::SLOT_1, next.clone(), size)
                .unwrap()
        );

        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, left);
        assert_eq!(scene.panes[ShellPaneId::SLOT_1].path, next);
        assert!(scene.pane_history(ShellPaneId::SLOT_0).back.is_empty());
        assert_eq!(
            scene.pane_history(ShellPaneId::SLOT_1).back,
            vec![right.clone()]
        );

        assert!(scene.go_history_back(size).unwrap());
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, left);
        assert_eq!(scene.panes[ShellPaneId::SLOT_1].path, right);
        assert!(scene.pane_history(ShellPaneId::SLOT_1).back.is_empty());
        assert_eq!(scene.pane_history(ShellPaneId::SLOT_1).forward, vec![next]);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn filter_and_hidden_rebuild_all_open_panes() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry(".left-hidden", false),
            ],
            ShellViewMode::Icons,
        );
        set_test_pane(
            &mut scene,
            ShellPaneId::SLOT_1,
            PathBuf::from("/right-root"),
            ShellViewMode::Icons,
            vec![
                test_entry("alpha-right.txt", false),
                test_entry(".right-hidden", false),
                test_entry("beta.txt", false),
            ],
        );
        let size = PhysicalSize::new(900, 360);

        assert_eq!(
            filtered_names(&scene, ShellPaneId::SLOT_0),
            vec!["alpha.txt"]
        );
        assert_eq!(
            filtered_names(&scene, ShellPaneId::SLOT_1),
            vec!["alpha-right.txt", "beta.txt"]
        );

        assert!(scene.toggle_hidden_visibility(size));
        assert_eq!(
            filtered_names(&scene, ShellPaneId::SLOT_0),
            vec!["alpha.txt", ".left-hidden"]
        );
        assert_eq!(
            filtered_names(&scene, ShellPaneId::SLOT_1),
            vec!["alpha-right.txt", ".right-hidden", "beta.txt"]
        );

        scene.panes[ShellPaneId::SLOT_1]
            .selection
            .select_indexes(&[1]);
        assert!(scene.apply_filter_command(FilterCommand::Activate, size));
        assert!(scene.apply_filter_command(FilterCommand::Insert("alpha".to_string()), size));
        assert_eq!(
            filtered_names(&scene, ShellPaneId::SLOT_0),
            vec!["alpha.txt"]
        );
        assert_eq!(
            filtered_names(&scene, ShellPaneId::SLOT_1),
            vec!["alpha-right.txt"]
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_1].selection.len(), 0);
    }

    #[test]
    fn pane_projection_shares_visible_items_and_scroll_metrics_across_panes() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Compact,
        );
        let split_entries = (0..24)
            .map(|index| test_entry(&format!("right-{index:02}"), index % 2 == 0))
            .collect::<Vec<_>>();
        scene.panes.set(
            ShellPaneId::SLOT_1,
            ShellPaneState {
                path: PathBuf::from("/right-root"),
                view_mode: ShellViewMode::Details,
                zoom_step: 0,
                dir_count: split_entries.iter().filter(|entry| entry.is_dir).count(),
                filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
                entries: split_entries,
                selection: ShellSelection::default(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        );
        let size = PhysicalSize::new(900, 360);

        let left = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        assert_eq!(left.geometry.kind, ShellPaneId::SLOT_0);
        assert_eq!(
            left.visible_items.len(),
            scene.layout(size).visible_items().len()
        );
        assert_eq!(left.scroll_metrics.max_scroll_x, scene.max_scroll_x(size));
        assert_eq!(left.scroll_metrics.max_scroll_y, scene.max_scroll_y(size));

        let split = scene.pane_projection(ShellPaneId::SLOT_1, size).unwrap();
        assert_eq!(split.geometry.kind, ShellPaneId::SLOT_1);
        assert_eq!(split.view.path, Path::new("/right-root"));
        assert!(!split.visible_items.is_empty());
        assert!(split.scroll_metrics.content_size.height >= split.geometry.content.height);
    }
