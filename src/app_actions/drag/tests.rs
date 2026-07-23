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

    assert_eq!(metrics.canvas_width, 188);
    assert_eq!(
        pixels.len(),
        (metrics.canvas_width * metrics.canvas_width * 4) as usize
    );
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
}

#[test]
fn outgoing_preview_metrics_follow_item_icon_size() {
    let metrics = outgoing_dnd_preview_metrics(64, 1.0);

    assert_eq!(metrics.icon_size, 64);
    assert_eq!(metrics.outline, 30);
    assert_eq!(metrics.canvas_width, 124);
}

#[test]
fn outgoing_preview_icon_size_preserves_scaled_source_size() {
    let source = ShellInternalDragPreviewSource::Place {
        label: "Home".to_string(),
        icon_name: "folder".to_string(),
        icon_size: 512.0,
    };

    assert_eq!(outgoing_dnd_preview_icon_size(Some(&source), 2.0), 512);
}

#[test]
fn outgoing_preview_metrics_align_fractional_scale_buffers() {
    let metrics = outgoing_dnd_preview_metrics(33, 1.5).with_label(true);

    assert_eq!(metrics.buffer_scale, 2);
    assert_eq!(metrics.icon_size, 44);
    assert_eq!(metrics.outline, 60);
    assert_eq!(metrics.canvas_width % metrics.buffer_scale as u32, 0);
    assert_eq!(metrics.canvas_height % metrics.buffer_scale as u32, 0);
    assert_eq!(metrics.icon_size % metrics.buffer_scale as u32, 0);
    assert!(metrics.label_rect.is_some());
}

#[test]
fn outgoing_places_preview_keeps_logical_geometry_across_ui_scales() {
    for (scale, expected_buffer_scale) in [(1.0, 1), (1.25, 1), (1.5, 2), (2.0, 2)] {
        let source_icon_size = (crate::PLACES_ICON_SIZE * scale).round() as u32;
        let metrics = outgoing_dnd_preview_metrics(source_icon_size, scale).with_label(true);
        let buffer_scale = metrics.buffer_scale as u32;

        assert_eq!(metrics.buffer_scale, expected_buffer_scale, "scale={scale}");
        assert_eq!(metrics.icon_size / buffer_scale, 22, "scale={scale}");
        assert_eq!(metrics.canvas_width / buffer_scale, 128, "scale={scale}");
        assert_eq!(metrics.canvas_height / buffer_scale, 118, "scale={scale}");
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
        assert_eq!(icon.offset_x, -64, "scale={scale}");
        assert_eq!(icon.offset_y, -41, "scale={scale}");
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
fn outgoing_preview_pixels_draw_label_below_icon() {
    let metrics = outgoing_dnd_preview_metrics(22, 1.0).with_label(true);
    let label_rect = metrics.label_rect.expect("label layout");
    let mut alpha = vec![0; (label_rect.width * label_rect.height) as usize];
    alpha[(label_rect.width / 2) as usize] = 255;
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
    let background_offset =
        ((label_rect.y as u32 * metrics.canvas_width + label_rect.width as u32 / 2) * 4) as usize;
    assert!(pixels[background_offset + 3] > 0);
    assert!(pixels[background_offset] > 20);
}

#[test]
fn outgoing_preview_pixels_add_badge_for_multiple_paths() {
    let metrics = outgoing_dnd_preview_metrics(128, 1.0);
    let single = outgoing_dnd_preview_pixels(&[PathBuf::from("/tmp/a.txt")], metrics, None);
    let multiple = outgoing_dnd_preview_pixels(
        &[PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")],
        metrics,
        None,
    );

    assert_ne!(single, multiple);
}

#[test]
fn outgoing_preview_pixels_use_supplied_icon_raster() {
    let metrics = outgoing_dnd_preview_metrics(128, 1.0);
    let raster = solid_test_raster(metrics.icon_size, [210, 32, 40, 255]);
    let preview = OutgoingDndPreviewRaster { icon: raster };
    let pixels =
        outgoing_dnd_preview_pixels(&[PathBuf::from("/tmp/a.txt")], metrics, Some(&preview));
    let center = metrics.outline as u32 + metrics.icon_size / 2;
    let offset = ((center * metrics.canvas_width + center) * 4) as usize;

    assert!(pixels[offset] > 160);
    assert!(pixels[offset + 1] < 80);
    assert!(pixels[offset + 2] < 90);
    assert!(pixels[offset + 3] > 180);
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
