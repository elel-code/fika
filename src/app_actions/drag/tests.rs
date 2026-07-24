use super::*;

#[test]
fn drop_sources_prefer_drop_event_paths() {
    let event_paths = vec![PathBuf::from("/tmp/drop.txt")];
    let tracked = Some(vec![PathBuf::from("/tmp/enter.txt")]);

    assert_eq!(
        external_drag_drop_sources(event_paths, tracked),
        vec![PathBuf::from("/tmp/drop.txt")]
    );
}

#[test]
fn drop_sources_fall_back_to_tracked_enter_paths() {
    let tracked = Some(vec![PathBuf::from("/tmp/enter.txt")]);

    assert_eq!(
        external_drag_drop_sources(Vec::new(), tracked),
        vec![PathBuf::from("/tmp/enter.txt")]
    );
}

#[test]
fn uri_list_data_decodes_file_paths() {
    assert_eq!(
        external_drag_paths_from_uris(vec!["file:///tmp/a%20file.txt".to_string()]),
        vec![PathBuf::from("/tmp/a file.txt")]
    );
}

#[test]
fn outgoing_payload_advertises_uri_list() {
    let payload = outgoing_dnd_payload(&[PathBuf::from("/tmp/a file.txt")]);

    assert_eq!(payload.uris, vec!["file:///tmp/a%20file.txt".to_string()]);
    assert_eq!(payload.text, "file:///tmp/a%20file.txt");
}

#[test]
fn outgoing_preview_pixels_are_sized_and_nonblank() {
    let metrics = outgoing_dnd_preview_metrics(128, 1.0);
    let pixels = outgoing_dnd_preview_pixels(&[PathBuf::from("/tmp/a.txt")], metrics, None);

    assert_eq!(metrics.canvas_width, 128);
    assert_eq!(
        pixels.len(),
        (metrics.canvas_width * metrics.canvas_height * 4) as usize
    );
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
}

#[test]
fn outgoing_preview_metrics_follow_item_icon_size() {
    let metrics = outgoing_dnd_preview_metrics(64, 1.0);

    assert_eq!(metrics.icon_size, 64);
    assert_eq!(metrics.cache_icon_size, 64.0);
    assert_eq!(metrics.icon_rect, PixelRect::new(0, 0, 64, 64));
    assert_eq!(metrics.canvas_width, 64);
}

#[test]
fn outgoing_preview_layout_preserves_scaled_source_size() {
    let layout = crate::shell::drag_preview_layout::place_single_drag_preview_layout(
        424.0,
        60.0,
        44.0,
        36.0,
        fika_core::ViewPoint { x: 88.0, y: 30.0 },
        2.0,
    );
    let metrics = outgoing_dnd_preview_metrics_for_layout(layout, 2.0);

    assert_eq!(metrics.icon_size, 44);
    // Scene-space icon size is what thumbnail/theme caches key on.
    assert_eq!(metrics.cache_icon_size, 44.0);
    assert_eq!(metrics.canvas_width, 424);
    assert_eq!(metrics.canvas_height, 60);
}

#[test]
fn outgoing_preview_cache_size_stays_scene_space_under_fractional_scale() {
    let layout = crate::shell::drag_preview_layout::pane_single_drag_preview_layout(
        crate::shell::options::ShellViewMode::Icons,
        None,
        96.0,
        80.0,
        20.0,
        1.5,
        None,
    );
    let metrics = outgoing_dnd_preview_metrics_for_layout(layout, 1.5);

    // Physical paint size grows with buffer scale; cache key stays scene-sized.
    assert!(metrics.icon_size >= 96);
    assert_eq!(metrics.cache_icon_size, 96.0);
}

#[test]
fn outgoing_preview_metrics_align_fractional_scale_buffers() {
    let layout = crate::shell::drag_preview_layout::place_single_drag_preview_layout(
        212.0 * 1.5,
        30.0 * 1.5,
        22.0 * 1.5,
        18.0 * 1.5,
        fika_core::ViewPoint {
            x: 44.0 * 1.5,
            y: 15.0 * 1.5,
        },
        1.5,
    );
    let metrics = outgoing_dnd_preview_metrics_for_layout(layout, 1.5);

    assert_eq!(metrics.buffer_scale, 2);
    assert_eq!(metrics.icon_size, 44);
    assert_eq!(metrics.canvas_width % metrics.buffer_scale as u32, 0);
    assert_eq!(metrics.canvas_height % metrics.buffer_scale as u32, 0);
    assert_eq!(metrics.icon_size % metrics.buffer_scale as u32, 0);
    assert!(metrics.label_rect.is_some());
}

#[test]
fn outgoing_fallback_preview_retains_a_name_without_source_metadata() {
    let metrics = outgoing_dnd_fallback_preview_metrics(1.25);
    let label_rect = metrics.label_rect.expect("fallback label layout");
    assert_eq!(
        metrics.label_style,
        Some(DragPreviewLabelStyle::PlainSingleLine)
    );
    assert!(label_rect.y >= metrics.icon_rect.bottom());
    assert_eq!(metrics.canvas_width % metrics.buffer_scale as u32, 0);
    assert_eq!(metrics.canvas_height % metrics.buffer_scale as u32, 0);

    let mut alpha = vec![0; (label_rect.width * label_rect.height) as usize];
    alpha[0] = 255;
    let label = OutgoingDndPreviewLabelRaster {
        alpha: Arc::from(alpha),
        width: label_rect.width as u32,
        height: label_rect.height as u32,
    };
    let pixels = outgoing_dnd_preview_pixels_with_label(
        &[PathBuf::from("/tmp/a.txt")],
        metrics,
        None,
        Some(&label),
        [20, 80, 160, 255],
    );
    let offset = ((label_rect.y as u32 * metrics.canvas_width + label_rect.x as u32) * 4) as usize;
    assert_eq!(pixels[offset + 3], 255);
}

#[test]
fn outgoing_places_preview_keeps_logical_geometry_across_ui_scales() {
    for (scale, expected_buffer_scale) in [(1.0, 1), (1.25, 1), (1.5, 2), (2.0, 2)] {
        let layout = crate::shell::drag_preview_layout::place_single_drag_preview_layout(
            212.0 * scale,
            30.0 * scale,
            crate::PLACES_ICON_SIZE * scale,
            18.0 * scale,
            fika_core::ViewPoint {
                x: 44.0 * scale,
                y: 15.0 * scale,
            },
            scale,
        );
        let metrics = outgoing_dnd_preview_metrics_for_layout(layout, scale);
        let buffer_scale = metrics.buffer_scale as u32;

        assert_eq!(metrics.buffer_scale, expected_buffer_scale, "scale={scale}");
        assert_eq!(metrics.icon_size / buffer_scale, 22, "scale={scale}");
        assert_eq!(metrics.canvas_width / buffer_scale, 212, "scale={scale}");
        assert_eq!(metrics.canvas_height / buffer_scale, 30, "scale={scale}");
        assert_eq!(metrics.canvas_width % buffer_scale, 0, "scale={scale}");
        assert_eq!(metrics.canvas_height % buffer_scale, 0, "scale={scale}");

        let icon = outgoing_dnd_drag_icon(
            &[PathBuf::from("/tmp/home")],
            metrics,
            None,
            None,
            [20, 80, 160, 230],
        )
        .expect("valid aligned drag icon");
        assert_eq!(icon.buffer_scale, expected_buffer_scale, "scale={scale}");
        assert_eq!(icon.offset_x, -44, "scale={scale}");
        assert_eq!(icon.offset_y, -15, "scale={scale}");
        wayland_client_runtime::DndIcon::new(
            icon.icon.rgba,
            icon.icon.width,
            icon.icon.height,
            icon.buffer_scale,
            wayland_client_runtime::LogicalPosition::new(icon.offset_x, icon.offset_y),
        )
        .expect("preview must satisfy the Wayland runtime contract");
    }
}

#[test]
fn outgoing_places_preview_draws_name_to_the_right_of_icon() {
    let layout = crate::shell::drag_preview_layout::place_single_drag_preview_layout(
        212.0,
        30.0,
        22.0,
        18.0,
        fika_core::ViewPoint { x: 44.0, y: 15.0 },
        1.0,
    );
    let metrics = outgoing_dnd_preview_metrics_for_layout(layout, 1.0);
    let label_rect = metrics.label_rect.expect("label layout");
    assert!(label_rect.x >= metrics.icon_rect.right());
    let mut alpha = vec![0; (label_rect.width * label_rect.height) as usize];
    alpha[0] = 255;
    let label = OutgoingDndPreviewLabelRaster {
        alpha: Arc::from(alpha),
        width: label_rect.width as u32,
        height: label_rect.height as u32,
    };
    let pixels = outgoing_dnd_preview_pixels_with_label(
        &[PathBuf::from("/tmp/a.txt")],
        metrics,
        None,
        Some(&label),
        [20, 80, 160, 230],
    );

    assert_eq!(
        pixels.len(),
        (metrics.canvas_width * metrics.canvas_height * 4) as usize
    );
    let label_offset =
        ((label_rect.y as u32 * metrics.canvas_width + label_rect.x as u32) * 4) as usize;
    assert_eq!(pixels[label_offset + 3], 230);
    assert!(pixels[label_offset + 2] > pixels[label_offset + 1]);
    assert!(pixels[label_offset + 1] > pixels[label_offset]);
}

#[test]
fn outgoing_multi_preview_draws_independent_grid_cells_without_a_label() {
    let layout = crate::shell::drag_preview_layout::multi_drag_preview_layout(4, 1.0);
    let metrics = outgoing_dnd_preview_metrics_for_multi_layout(layout, 1.0);
    let rasters = OutgoingDndPreviewRasters {
        icons: vec![
            Some(solid_test_raster(metrics.icon_size, [210, 32, 40, 255])),
            Some(solid_test_raster(metrics.icon_size, [32, 180, 70, 255])),
            Some(solid_test_raster(metrics.icon_size, [40, 80, 220, 255])),
            Some(solid_test_raster(metrics.icon_size, [220, 180, 30, 255])),
        ],
    };
    let paths = [
        PathBuf::from("/tmp/a.txt"),
        PathBuf::from("/tmp/b.txt"),
        PathBuf::from("/tmp/c.txt"),
        PathBuf::from("/tmp/d.txt"),
    ];
    let pixels = outgoing_dnd_preview_pixels(&paths, metrics, Some(&rasters));

    assert_eq!(metrics.visible_icon_count(), 4);
    assert!(metrics.label_rect.is_none());
    assert_eq!((metrics.canvas_width, metrics.canvas_height), (99, 66));
    assert_pixel_near(
        &pixels,
        metrics,
        metrics.icon_rect_at(0).unwrap(),
        [210, 32, 40],
    );
    assert_pixel_near(
        &pixels,
        metrics,
        metrics.icon_rect_at(3).unwrap(),
        [220, 180, 30],
    );
}

#[test]
fn outgoing_multi_preview_caps_at_dolphins_five_by_five_grid() {
    for (scale, expected_buffer_scale) in [(1.0, 1), (1.25, 1), (1.5, 2), (2.0, 2)] {
        let layout = crate::shell::drag_preview_layout::multi_drag_preview_layout(30, scale);
        let metrics = outgoing_dnd_preview_metrics_for_multi_layout(layout, scale);
        let buffer_scale = metrics.buffer_scale as u32;
        assert_eq!(metrics.visible_icon_count(), 25, "scale={scale}");
        assert_eq!(metrics.buffer_scale, expected_buffer_scale, "scale={scale}");
        assert_eq!(metrics.icon_size / buffer_scale, 16, "scale={scale}");
        assert_eq!(metrics.canvas_width / buffer_scale, 85, "scale={scale}");
        assert_eq!(metrics.canvas_height / buffer_scale, 85, "scale={scale}");
        assert_eq!(metrics.hotspot_x / buffer_scale as i32, 42, "scale={scale}");
        assert_eq!(metrics.hotspot_y, 0, "scale={scale}");

        let icon = outgoing_dnd_drag_icon(
            &(0..30)
                .map(|index| PathBuf::from(format!("/tmp/item-{index}")))
                .collect::<Vec<_>>(),
            metrics,
            None,
            None,
            [0, 0, 0, 0],
        )
        .expect("valid aligned grid drag icon");
        assert_eq!(icon.offset_x, -42, "scale={scale}");
        assert_eq!(icon.offset_y, 0, "scale={scale}");
        wayland_client_runtime::DndIcon::new(
            icon.icon.rgba,
            icon.icon.width,
            icon.icon.height,
            icon.buffer_scale,
            wayland_client_runtime::LogicalPosition::new(icon.offset_x, icon.offset_y),
        )
        .expect("grid preview must satisfy the Wayland runtime contract");
    }
}

#[test]
fn outgoing_preview_pixels_use_supplied_icon_raster() {
    let metrics = outgoing_dnd_preview_metrics(128, 1.0);
    let raster = solid_test_raster(metrics.icon_size, [210, 32, 40, 255]);
    let preview = OutgoingDndPreviewRasters {
        icons: vec![Some(raster)],
    };
    let pixels =
        outgoing_dnd_preview_pixels(&[PathBuf::from("/tmp/a.txt")], metrics, Some(&preview));
    let center_x = metrics.icon_rect.x as u32 + metrics.icon_size / 2;
    let center_y = metrics.icon_rect.y as u32 + metrics.icon_size / 2;
    let offset = ((center_y * metrics.canvas_width + center_x) * 4) as usize;

    assert!(pixels[offset] > 160);
    assert!(pixels[offset + 1] < 80);
    assert!(pixels[offset + 2] < 90);
    assert!(pixels[offset + 3] > 180);
}

fn assert_pixel_near(
    pixels: &[u8],
    metrics: OutgoingDndPreviewMetrics,
    rect: PixelRect,
    expected: [u8; 3],
) {
    let x = rect.x as u32 + rect.width as u32 / 2;
    let y = rect.y as u32 + rect.height as u32 / 2;
    let offset = ((y * metrics.canvas_width + x) * 4) as usize;
    for channel in 0..3 {
        assert!(pixels[offset + channel].abs_diff(expected[channel]) <= 2);
    }
    assert_eq!(pixels[offset + 3], 255);
}

#[cfg(unix)]
#[test]
fn drag_emblem_kinds_include_link_for_symlink() {
    let dir = std::env::temp_dir().join(format!("fika-dnd-link-emblem-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("target.txt");
    let link = dir.join("link.txt");
    fs::write(&target, "x").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    assert!(icon_emblem_kinds_for_path(&link).contains(&crate::IconEmblemKind::Link));

    fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn drag_emblem_kinds_skip_marker_for_readable_unwritable_file() {
    let dir = std::env::temp_dir().join(format!("fika-dnd-readonly-emblem-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("readonly.txt");
    fs::write(&path, "x").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o444);
    fs::set_permissions(&path, permissions).unwrap();

    assert!(icon_emblem_kinds_for_path(&path).is_empty());

    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o644);
    fs::set_permissions(&path, permissions).unwrap();
    fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn drag_emblem_kinds_prefer_locked_for_unreadable_file() {
    let dir =
        std::env::temp_dir().join(format!("fika-dnd-unreadable-emblem-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("unreadable.txt");
    fs::write(&path, "x").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(&path, permissions).unwrap();

    let emblems = icon_emblem_kinds_for_path(&path);
    assert!(emblems.contains(&crate::IconEmblemKind::Unreadable));

    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o644);
    fs::set_permissions(&path, permissions).unwrap();
    fs::remove_dir_all(&dir).unwrap();
}

fn solid_test_raster(size: u32, color: [u8; 4]) -> IconRaster {
    let mut pixels = vec![0; (size * size * 4) as usize];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&color);
    }
    IconRaster {
        pixels: Arc::from(pixels),
        width: size,
        height: size,
    }
}
