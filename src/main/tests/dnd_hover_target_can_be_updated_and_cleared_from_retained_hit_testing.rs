
    #[test]
    fn dnd_hover_target_can_be_updated_and_cleared_from_retained_hit_testing() {
        let mut scene = test_scene(
            vec![test_entry("alpha", true), test_entry("note.txt", false)],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let content = projection.geometry.content;
        let item = projection.visible_items[0];
        let item_point = ViewPoint {
            x: content.x + item.layout.visual_rect.x + 4.0,
            y: content.y + item.layout.visual_rect.y + 4.0,
        };

        assert!(scene.begin_internal_drag_for_pane_item(ShellPaneId::SLOT_0, 1, item_point));
        if let Some(drag) = scene.internal_drag.as_mut() {
            drag.active = true;
        }
        assert!(scene.update_dnd_hover_target(item_point, size));
        assert_eq!(
            scene.dnd_hover_target.as_ref().map(ShellDropTarget::kind),
            Some("pane-item")
        );
        assert_eq!(scene.dnd_hover_changes, 1);
        assert!(!scene.update_dnd_hover_target(item_point, size));
        assert_eq!(scene.dnd_hover_changes, 1);

        let blank_point = ViewPoint {
            x: content.right() - 2.0,
            y: content.bottom() - 2.0,
        };
        assert!(scene.update_dnd_hover_target(blank_point, size));
        assert_eq!(
            scene.dnd_hover_target.as_ref().map(ShellDropTarget::kind),
            Some("pane-blank")
        );
        assert_eq!(scene.dnd_hover_changes, 2);
        assert!(scene.clear_dnd_hover_target());
        assert_eq!(scene.dnd_hover_target, None);
        assert_eq!(scene.dnd_hover_changes, 3);
    }

    #[test]
    fn internal_drag_to_slot0_blank_creates_copy_drop_request() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("beta.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let item = projection.visible_items[0];
        let start = ViewPoint {
            x: projection.geometry.content.x + item.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + item.layout.visual_rect.y + 6.0,
        };
        let blank = ViewPoint {
            x: projection.geometry.content.right() - 4.0,
            y: projection.geometry.content.bottom() - 4.0,
        };

        assert!(scene.begin_pane_pointer(
            SelectionClick {
                point: start,
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert!(scene.set_pointer(blank, size));
        assert!(scene.internal_drag.as_ref().is_some_and(|drag| drag.active));
        assert!(scene.end_pane_pointer(blank, size));

        assert!(scene.pending_drop_request.is_none());
        let menu = scene
            .drop_menu
            .as_ref()
            .expect("active internal drag should open a drop menu");
        assert_eq!(menu.sources, vec![PathBuf::from("/tmp/alpha.txt")]);
        assert_eq!(menu.target_dir, PathBuf::from("/tmp"));
        let copy_row = drop_menu_rect(menu, size);
        let request = scene
            .activate_or_close_drop_menu_request(
                ViewPoint {
                    x: copy_row.x + 8.0,
                    y: copy_row.y + CONTEXT_MENU_VERTICAL_PADDING + 2.0,
                },
                size,
            )
            .expect("copy row should create a drop request");
        assert_eq!(request.sources, vec![PathBuf::from("/tmp/alpha.txt")]);
        assert_eq!(request.target_dir, PathBuf::from("/tmp"));
        assert_eq!(
            request.target,
            ShellDropTarget::PaneBlank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            }
        );
        assert_eq!(request.mode, FileTransferMode::Copy);
        assert_eq!(scene.dnd_drop_requests, 1);
        assert_eq!(scene.pending_drop_request.as_ref(), Some(&request));
        assert!(scene.internal_drag.is_none());
        assert!(scene.dnd_hover_target.is_none());
    }

    #[test]
    fn external_drag_to_pane_blank_opens_drop_menu() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let blank = ViewPoint {
            x: projection.geometry.content.right() - 4.0,
            y: projection.geometry.content.bottom() - 4.0,
        };
        let source = PathBuf::from("/external/source.txt");

        assert!(scene.begin_data_transfer_drag(
            vec![source.clone(), source.clone(), PathBuf::new()],
            None,
            blank,
            size,
        ));
        assert_eq!(
            scene.dnd_hover_target,
            Some(ShellDropTarget::PaneBlank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp")
            })
        );
        assert_eq!(
            scene
                .external_drag
                .as_ref()
                .map(|drag| drag.sources.as_slice()),
            Some([source.clone()].as_slice())
        );

        assert!(
            scene
                .finish_external_drag(vec![source.clone(), source.clone()], blank, size)
                .unwrap()
        );
        let menu = scene
            .drop_menu
            .as_ref()
            .expect("external drop should open a drop menu");
        assert_eq!(menu.sources, vec![source]);
        assert_eq!(menu.target_dir, PathBuf::from("/tmp"));
        assert!(matches!(menu.target, ShellDropTarget::PaneBlank { .. }));
        assert!(scene.external_drag.is_none());
        assert!(scene.dnd_hover_target.is_none());
    }

    #[test]
    fn external_drag_to_directory_item_targets_that_directory() {
        let mut scene = test_scene(
            vec![test_entry("folder", true), test_entry("note.txt", false)],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let folder = projection.visible_items[0];
        let target = ViewPoint {
            x: projection.geometry.content.x + folder.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + folder.layout.visual_rect.y + 6.0,
        };
        let source = PathBuf::from("/external/source.txt");

        assert!(scene.begin_data_transfer_drag(vec![source.clone()], None, target, size));
        assert_eq!(
            scene.dnd_hover_target,
            Some(ShellDropTarget::PaneItem {
                pane: ShellPaneId::SLOT_0,
                index: 0,
                path: PathBuf::from("/tmp/folder"),
                is_dir: true,
            })
        );
        assert!(
            scene
                .finish_external_drag(vec![source.clone()], target, size)
                .unwrap()
        );
        let menu = scene.drop_menu.as_ref().unwrap();
        assert_eq!(menu.sources, vec![source]);
        assert_eq!(menu.target_dir, PathBuf::from("/tmp/folder"));
    }

    #[test]
    fn details_drop_target_uses_full_row_when_selected() {
        let mut scene = test_scene(vec![test_entry("folder", true)], ShellViewMode::Details);
        let size = PhysicalSize::new(900, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let content = projection.geometry.content;
        let row = projection.visible_items[0].layout;
        let blank_side = ViewPoint {
            x: content.right() - 4.0,
            y: content.y + row.item_rect.y + row.item_rect.height / 2.0,
        };

        assert_eq!(scene.hit_test_screen_point(blank_side, size), Some(0));
        assert_eq!(
            scene.drop_target_at_screen_point(blank_side, size),
            Some(ShellDropTarget::PaneBlank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            })
        );

        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .apply_navigation(0, false)
        );
        assert_eq!(
            scene.drop_target_at_screen_point(blank_side, size),
            Some(ShellDropTarget::PaneItem {
                pane: ShellPaneId::SLOT_0,
                index: 0,
                path: PathBuf::from("/tmp/folder"),
                is_dir: true,
            })
        );
    }

    #[test]
    fn external_drag_rejects_plain_files_and_clears_hover() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let item = projection.visible_items[0];
        let target = ViewPoint {
            x: projection.geometry.content.x + item.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + item.layout.visual_rect.y + 6.0,
        };
        let source = PathBuf::from("/external/source.txt");

        assert!(scene.begin_data_transfer_drag(vec![source.clone()], None, target, size));
        assert_eq!(scene.dnd_hover_target, None);
        assert!(
            scene
                .finish_external_drag(vec![source], target, size)
                .unwrap()
        );
        assert!(scene.drop_menu.is_none());
        assert!(scene.external_drag.is_none());
        assert!(scene.dnd_hover_target.is_none());
    }

    #[test]
    fn drop_menu_administrator_rows_create_privileged_requests() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.drop_menu = Some(ShellDropMenu::new(
            vec![PathBuf::from("/tmp/alpha.txt")],
            PathBuf::from("/etc"),
            ShellDropTarget::PaneBlank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/etc"),
            },
            ViewPoint { x: 80.0, y: 80.0 },
        ));
        let menu = scene.drop_menu.as_ref().unwrap();
        let rect = drop_menu_rect(menu, size);
        let admin_copy_index = drop_menu_items()
            .iter()
            .position(|item| {
                item.command
                    == ShellDropMenuCommand::Mode {
                        mode: FileTransferMode::Copy,
                        privileged: true,
                    }
            })
            .unwrap();
        let request = scene
            .activate_or_close_drop_menu_request(
                ViewPoint {
                    x: rect.x + 8.0,
                    y: rect.y
                        + CONTEXT_MENU_VERTICAL_PADDING
                        + CONTEXT_MENU_ROW_HEIGHT * admin_copy_index as f32
                        + 2.0,
                },
                size,
            )
            .expect("administrator copy row should create a drop request");
        assert_eq!(request.mode, FileTransferMode::Copy);
        assert!(request.privileged);
        assert_eq!(scene.pending_drop_request.as_ref(), Some(&request));
    }

    #[test]
    fn internal_drag_to_directory_item_targets_that_directory() {
        let mut scene = test_scene(
            vec![test_entry("folder", true), test_entry("note.txt", false)],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let folder = projection.visible_items[0];
        let note = projection.visible_items[1];
        let start = ViewPoint {
            x: projection.geometry.content.x + note.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + note.layout.visual_rect.y + 6.0,
        };
        let target = ViewPoint {
            x: projection.geometry.content.x + folder.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + folder.layout.visual_rect.y + 6.0,
        };

        assert!(scene.begin_pane_pointer(
            SelectionClick {
                point: start,
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert!(scene.set_pointer(target, size));
        assert!(scene.end_pane_pointer(target, size));

        let menu = scene
            .drop_menu
            .as_ref()
            .expect("directory target should open a drop menu");
        assert_eq!(menu.sources, vec![PathBuf::from("/tmp/note.txt")]);
        assert_eq!(menu.target_dir, PathBuf::from("/tmp/folder"));
        let rect = drop_menu_rect(menu, size);
        let request = scene
            .activate_or_close_drop_menu_request(
                ViewPoint {
                    x: rect.x + 8.0,
                    y: rect.y + CONTEXT_MENU_VERTICAL_PADDING + 2.0,
                },
                size,
            )
            .expect("copy row should create a drop request");
        assert_eq!(request.sources, vec![PathBuf::from("/tmp/note.txt")]);
        assert_eq!(request.target_dir, PathBuf::from("/tmp/folder"));
        assert_eq!(
            request.target,
            ShellDropTarget::PaneItem {
                pane: ShellPaneId::SLOT_0,
                index: 0,
                path: PathBuf::from("/tmp/folder"),
                is_dir: true,
            }
        );
        assert_eq!(scene.dnd_drop_requests, 1);
    }

    #[test]
    fn internal_drag_below_threshold_finishes_as_plain_click() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let item = projection.visible_items[0];
        let start = ViewPoint {
            x: projection.geometry.content.x + item.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + item.layout.visual_rect.y + 6.0,
        };
        let end = ViewPoint {
            x: start.x + 1.0,
            y: start.y + 1.0,
        };

        assert!(scene.begin_pane_pointer(
            SelectionClick {
                point: start,
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert!(scene.set_pointer(end, size));
        assert!(!scene.internal_drag.as_ref().is_some_and(|drag| drag.active));
        assert!(!scene.end_pane_pointer(end, size));
        assert!(scene.pending_drop_request.is_none());
        assert_eq!(scene.dnd_drop_requests, 0);
        assert!(scene.panes[ShellPaneId::SLOT_0].selection.contains(0));
    }

    #[test]
    fn internal_drag_to_plain_file_clears_hover_without_drop_request() {
        let mut scene = test_scene(
            vec![
                test_entry("source.txt", false),
                test_entry("target.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let source = projection.visible_items[0];
        let target = projection.visible_items[1];
        let start = ViewPoint {
            x: projection.geometry.content.x + source.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + source.layout.visual_rect.y + 6.0,
        };
        let end = ViewPoint {
            x: projection.geometry.content.x + target.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + target.layout.visual_rect.y + 6.0,
        };

        assert!(scene.begin_pane_pointer(
            SelectionClick {
                point: start,
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert!(scene.set_pointer(end, size));
        assert_eq!(
            scene.dnd_hover_target.as_ref().map(ShellDropTarget::kind),
            None
        );
        assert!(scene.end_pane_pointer(end, size));
        assert!(scene.pending_drop_request.is_none());
        assert_eq!(scene.dnd_drop_requests, 0);
        assert!(scene.dnd_hover_target.is_none());
    }

    #[test]
    fn place_drag_to_places_gap_reorders_and_persists_order() {
        let root = test_dir("place-dnd-reorder");
        let places_path = root.join("places.xbel");
        let alpha = PathBuf::from("/tmp/place-alpha");
        let beta = PathBuf::from("/tmp/place-beta");
        let gamma = PathBuf::from("/tmp/place-gamma");
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![
            ShellPlace::new("", "A", "Alpha", alpha.clone(), true),
            ShellPlace::new("", "B", "Beta", beta.clone(), true),
            ShellPlace::new("", "G", "Gamma", gamma.clone(), true),
        ];
        let size = PhysicalSize::new(700, 360);
        let start = scene.place_row_rects(size)[0].1;
        let gap = scene
            .place_gap_rect_for_index(3, size)
            .expect("last gap should be visible");
        let start_point = ViewPoint {
            x: start.x + 6.0,
            y: start.y + 6.0,
        };
        let gap_point = ViewPoint {
            x: gap.x + gap.width / 2.0,
            y: gap.y + gap.height / 2.0,
        };

        assert!(scene.begin_internal_drag_for_place(0, start_point));
        assert!(scene.set_pointer(gap_point, size));
        assert_eq!(
            scene.dnd_hover_target,
            Some(ShellDropTarget::PlacesGap { index: 3 })
        );
        assert!(
            scene
                .finish_internal_drag_with_user_places_path(gap_point, size, &places_path)
                .unwrap()
        );

        assert_eq!(
            scene
                .places
                .iter()
                .map(|place| place.path.clone())
                .collect::<Vec<_>>(),
            vec![beta.clone(), gamma.clone(), alpha.clone()]
        );
        assert_eq!(
            load_place_order(&place_order_path_for_user_places_path(&places_path)).unwrap(),
            vec![beta, gamma, alpha]
        );
        fs::remove_dir_all(root).unwrap();
    }
