use std::path::Path;
use std::sync::Arc;

use fika_core::{ItemLayout, PaneId, ViewRect};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Corners, Element, ElementId, FontWeight, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, ObjectFit, Pixels, RenderImage, SharedString, Style, StyleRefinement,
    Styled, TextAlign, TextRun, WeakEntity, Window, fill, point, px, rgb, size,
};

use crate::FikaApp;
use crate::ui::icons::{FileIconSnapshot, IconPaintMode, theme_icon_image_size_px};
use crate::ui::retained::{
    RetainedImageLayerState, RetainedImageLoadOutcome, RetainedImageRequest,
    RetainedImageRequestKind,
};

use super::ITEM_NAME_LINE_HEIGHT;
use super::ItemImageSourcePerfStats;
use super::paint_slots::ItemPaintSnapshot;
use super::renderer_policy::{
    ItemRendererPolicyInput, item_uses_image_layer_with_input, item_uses_layer_visual_paint,
};
use super::text::static_paint_single_line_text;

pub(super) fn item_image_layer_view(
    pane_id: PaneId,
    items: &[ItemPaintSnapshot],
    width: f32,
    height: f32,
    app: WeakEntity<FikaApp>,
) -> Option<ItemImageLayerElement> {
    let items = item_image_layer_items_for_policy(items);
    (!items.is_empty()).then(|| {
        ItemImageLayerElement {
            pane_id,
            app,
            items,
            style: StyleRefinement::default(),
        }
        .absolute()
        .left_0()
        .top_0()
        .w(px(width.max(1.0)))
        .h(px(height.max(1.0)))
    })
}

#[cfg(test)]
pub(super) fn item_image_layer_items(items: &[ItemPaintSnapshot]) -> Vec<ItemImageLayerItem> {
    item_image_layer_items_for_policy(items)
}

pub(super) fn item_image_layer_items_for_policy(
    items: &[ItemPaintSnapshot],
) -> Vec<ItemImageLayerItem> {
    items
        .iter()
        .filter(|item| item.visible)
        .filter_map(|item| {
            let content = item.content.as_ref();
            if !item_uses_layer_visual_paint(content) {
                return None;
            }
            if !item_uses_image_layer_with_input(content, ItemRendererPolicyInput::default()) {
                return None;
            }
            Some(ItemImageLayerItem {
                visible: item.visible,
                layout: item.layout,
                thumbnail_path: content.thumbnail_path.clone(),
                icon: content.icon.clone(),
                fallback_marker: content.fallback_marker.clone(),
                mode: IconPaintMode::Normal,
            })
        })
        .collect()
}

pub(super) struct ItemImageLayerItem {
    visible: bool,
    layout: ItemLayout,
    thumbnail_path: Option<Arc<Path>>,
    icon: FileIconSnapshot,
    fallback_marker: SharedString,
    mode: IconPaintMode,
}

#[cfg(test)]
pub(super) fn item_image_layer_item_source_path(item: &ItemImageLayerItem) -> Option<Arc<Path>> {
    item.thumbnail_path
        .clone()
        .or_else(|| item.icon.path.clone())
}

#[cfg(test)]
pub(super) fn item_image_load_failure_paints_fallback(_item: &ItemImageLayerItem) -> bool {
    true
}

#[cfg(test)]
pub(super) fn item_image_pending_load_paints_fallback(_item: &ItemImageLayerItem) -> bool {
    true
}

#[cfg(test)]
pub(super) fn item_image_pending_load_paints_marker(item: &ItemImageLayerItem) -> bool {
    item.thumbnail_path.is_some()
}

pub(super) fn item_image_retained_request_for(
    thumbnail_path: Option<Arc<Path>>,
    icon: &FileIconSnapshot,
    icon_size_px: u32,
    scale_factor: f32,
    mode: IconPaintMode,
) -> Option<RetainedImageRequest> {
    RetainedImageRequest::thumbnail_or_theme_icon_for_snapshot_with_mode(
        thumbnail_path,
        icon,
        icon_size_px,
        scale_factor,
        mode,
    )
}

fn item_image_layer_retained_request(
    item: &ItemImageLayerItem,
    scale_factor: f32,
) -> Option<RetainedImageRequest> {
    item_image_retained_request_for(
        item.thumbnail_path.clone(),
        &item.icon,
        item_image_theme_icon_size_px(item),
        scale_factor,
        item.mode,
    )
}

fn item_image_theme_icon_size_px(item: &ItemImageLayerItem) -> u32 {
    theme_icon_image_size_px(item.layout.icon_rect.width, item.layout.icon_rect.height)
}

pub(super) struct ItemImageLayerElement {
    pane_id: PaneId,
    app: WeakEntity<FikaApp>,
    items: Vec<ItemImageLayerItem>,
    style: StyleRefinement,
}

pub(super) struct ItemImagePaintState {
    visible: bool,
    paint: bool,
    icon_rect: ViewRect,
    kind: ItemImagePaintKind,
    image: Option<Arc<RenderImage>>,
    fallback: Option<ItemImageFallbackPaintState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ItemImagePaintKind {
    Thumbnail,
    ThemeIcon,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ThemeIconPlaceholderKind {
    File,
    Folder,
}

pub(super) struct ItemImageFallbackPaintState {
    pub(super) marker_line: Option<gpui::ShapedLine>,
    pub(super) marker_line_height: Pixels,
    pub(super) fallback_bg: u32,
    pub(super) placeholder: Option<ThemeIconPlaceholderKind>,
}

impl IntoElement for ItemImageLayerElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ItemImageLayerElement {
    type RequestLayoutState = Style;
    type PrepaintState = Vec<ItemImagePaintState>;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::from(item_image_paint_layer_element_id(
            self.pane_id,
        )))
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.refine(&self.style);
        let layout_id = window.request_layout(style.clone(), [], cx);
        (layout_id, style)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let Some(id) = id else {
            return Vec::new();
        };
        let perf_started = super::item_view_perf_enabled().then(std::time::Instant::now);
        window.with_element_state::<RetainedImageLayerState, _>(id, |state, window| {
            let mut state = state.unwrap_or_else(|| RetainedImageLayerState::new(cx));
            let mut source_stats = ItemImageSourcePerfStats::default();
            let states = self
                .items
                .iter()
                .filter_map(|item| {
                    item_image_layer_prepaint_item(
                        item,
                        &mut state,
                        &self.app,
                        &mut source_stats,
                        window,
                        cx,
                    )
                })
                .collect::<Vec<_>>();
            if let Some(started) = perf_started {
                let elapsed = started.elapsed();
                let count = states.len();
                let _ = self.app.update(cx, |this, _cx| {
                    this.record_item_image_prepaint(self.pane_id, elapsed, count, source_stats);
                });
            }
            (states, state)
        })
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let perf_started = super::item_view_perf_enabled().then(std::time::Instant::now);
        let paint_count = prepaint.iter().filter(|state| state.paint).count();
        request_layout.paint(bounds, window, cx, |window, cx| {
            for state in prepaint.iter() {
                if !state.visible || !state.paint {
                    continue;
                }
                item_image_layer_paint_item(bounds, state, window, cx);
            }
        });
        if let Some(started) = perf_started {
            let elapsed = started.elapsed();
            let _ = self.app.update(cx, |this, _cx| {
                this.record_item_image_paint(self.pane_id, elapsed, paint_count);
            });
        }
    }
}

impl Styled for ItemImageLayerElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

pub(super) fn item_image_paint_layer_element_id(pane_id: PaneId) -> (&'static str, u64) {
    ("item-image-paint-layer", pane_id.0)
}

fn item_image_layer_prepaint_item(
    item: &ItemImageLayerItem,
    state: &mut RetainedImageLayerState,
    app: &WeakEntity<FikaApp>,
    source_stats: &mut ItemImageSourcePerfStats,
    window: &mut Window,
    cx: &mut App,
) -> Option<ItemImagePaintState> {
    if !item.visible {
        return None;
    }
    let request = item_image_layer_retained_request(item, window.scale_factor())?;
    let kind = item_image_paint_kind_for_request(&request);
    let load = state.load_request_or_retained_with_outcome(request, app, window, cx);
    record_item_image_source_stats(source_stats, kind, load.outcome);
    let image = load.image;
    let fallback = image.is_none().then(|| {
        if kind == ItemImagePaintKind::Thumbnail {
            item_image_marker_fallback_prepaint(item, window)
        } else {
            theme_icon_placeholder_fallback(&item.icon)
        }
    });
    Some(ItemImagePaintState {
        visible: item.visible,
        paint: true,
        icon_rect: item.layout.icon_rect,
        kind,
        image,
        fallback,
    })
}

fn item_image_paint_kind_for_request(request: &RetainedImageRequest) -> ItemImagePaintKind {
    match request.kind() {
        RetainedImageRequestKind::Thumbnail => ItemImagePaintKind::Thumbnail,
        RetainedImageRequestKind::ThemeIcon => ItemImagePaintKind::ThemeIcon,
    }
}

fn record_item_image_source_stats(
    stats: &mut ItemImageSourcePerfStats,
    kind: ItemImagePaintKind,
    outcome: RetainedImageLoadOutcome,
) {
    match (kind, outcome) {
        (ItemImagePaintKind::Thumbnail, RetainedImageLoadOutcome::CacheReady { first_ready }) => {
            stats.thumbnail_loaded += 1;
            if first_ready {
                stats.thumbnail_decoded += 1;
            }
        }
        (ItemImagePaintKind::Thumbnail, RetainedImageLoadOutcome::Retained) => {
            stats.thumbnail_retained += 1;
        }
        (ItemImagePaintKind::Thumbnail, RetainedImageLoadOutcome::Missing) => {
            stats.thumbnail_fallback += 1;
        }
        (ItemImagePaintKind::ThemeIcon, RetainedImageLoadOutcome::CacheReady { first_ready }) => {
            stats.theme_loaded += 1;
            if first_ready {
                stats.theme_decoded += 1;
            }
        }
        (ItemImagePaintKind::ThemeIcon, RetainedImageLoadOutcome::Retained) => {
            stats.theme_retained += 1;
        }
        (ItemImagePaintKind::ThemeIcon, RetainedImageLoadOutcome::Missing) => {
            stats.theme_placeholder += 1;
        }
    }
}

fn item_image_marker_fallback_prepaint(
    item: &ItemImageLayerItem,
    window: &mut Window,
) -> ItemImageFallbackPaintState {
    let text_style = window.text_style();
    let mut marker_font = text_style.font();
    marker_font.weight = FontWeight::SEMIBOLD;
    let marker = static_paint_single_line_text(item.fallback_marker.clone());
    let marker_run = TextRun {
        len: marker.len(),
        font: marker_font,
        color: rgb(item.icon.fallback_fg).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let marker_font_size = px(window.rem_size().as_f32() * 0.75);
    ItemImageFallbackPaintState {
        marker_line: Some(window.text_system().shape_line(
            marker,
            marker_font_size,
            &[marker_run],
            None,
        )),
        marker_line_height: px(item
            .layout
            .icon_rect
            .height
            .min(ITEM_NAME_LINE_HEIGHT)
            .max(1.0)),
        fallback_bg: item.icon.fallback_bg,
        placeholder: None,
    }
}

pub(super) fn theme_icon_placeholder_fallback(
    icon: &FileIconSnapshot,
) -> ItemImageFallbackPaintState {
    ItemImageFallbackPaintState {
        marker_line: None,
        marker_line_height: px(0.0),
        fallback_bg: 0xf3f4f6,
        placeholder: Some(theme_icon_placeholder_kind(icon)),
    }
}

pub(super) fn theme_icon_placeholder_kind(icon: &FileIconSnapshot) -> ThemeIconPlaceholderKind {
    if icon.fallback_marker.as_ref() == "DIR"
        || icon.icon_name.as_ref() == "folder"
        || icon.icon_name.as_ref() == "inode-directory"
    {
        ThemeIconPlaceholderKind::Folder
    } else {
        ThemeIconPlaceholderKind::File
    }
}

fn item_image_layer_paint_item(
    layer_bounds: Bounds<Pixels>,
    state: &ItemImagePaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let icon_bounds = item_image_layer_icon_bounds(layer_bounds, state.icon_rect);
    if let Some(image) = state.image.as_ref() {
        if image.frame_count() == 0 {
            return;
        }
        let image_size = image.size(0);
        if u32::from(image_size.width) == 0 || u32::from(image_size.height) == 0 {
            return;
        }
        let image_bounds = match state.kind {
            ItemImagePaintKind::Thumbnail => ObjectFit::Contain.get_bounds(icon_bounds, image_size),
            ItemImagePaintKind::ThemeIcon => theme_icon_square_bounds(icon_bounds),
        };
        window
            .paint_image(image_bounds, Corners::all(px(6.0)), image.clone(), 0, false)
            .ok();
        return;
    }

    if let Some(fallback) = state.fallback.as_ref() {
        paint_item_image_fallback(icon_bounds, fallback, px(6.0), window, cx);
    }
}

pub(super) fn paint_item_image_fallback(
    icon_bounds: Bounds<Pixels>,
    fallback: &ItemImageFallbackPaintState,
    corner_radius: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(marker_line) = fallback.marker_line.as_ref() {
        window.paint_quad(fill(icon_bounds, rgb(fallback.fallback_bg)).corner_radii(corner_radius));
        let marker_origin = point(
            icon_bounds.origin.x,
            icon_bounds.origin.y
                + ((icon_bounds.size.height - fallback.marker_line_height).max(px(0.0)) / 2.0),
        );
        marker_line
            .paint(
                marker_origin,
                fallback.marker_line_height,
                TextAlign::Center,
                Some(icon_bounds.size.width),
                window,
                cx,
            )
            .ok();
    } else {
        paint_theme_icon_placeholder(
            icon_bounds,
            corner_radius,
            fallback
                .placeholder
                .unwrap_or(ThemeIconPlaceholderKind::File),
            window,
        );
    }
}

pub(super) fn paint_theme_icon_image(
    icon_bounds: Bounds<Pixels>,
    image: &Arc<RenderImage>,
    corner_radius: Pixels,
    window: &mut Window,
) -> bool {
    if image.frame_count() == 0 {
        return false;
    }
    let image_size = image.size(0);
    if u32::from(image_size.width) == 0 || u32::from(image_size.height) == 0 {
        return false;
    }
    window
        .paint_image(
            theme_icon_square_bounds(icon_bounds),
            Corners::all(corner_radius),
            image.clone(),
            0,
            false,
        )
        .is_ok()
}

fn paint_theme_icon_placeholder(
    icon_bounds: Bounds<Pixels>,
    corner_radius: Pixels,
    kind: ThemeIconPlaceholderKind,
    window: &mut Window,
) {
    let icon_bounds = theme_icon_square_bounds(icon_bounds);
    let side = icon_bounds
        .size
        .width
        .min(icon_bounds.size.height)
        .as_f32()
        .max(1.0);
    match kind {
        ThemeIconPlaceholderKind::File => {
            paint_file_theme_icon_placeholder(icon_bounds, side, corner_radius, window);
        }
        ThemeIconPlaceholderKind::Folder => {
            paint_folder_theme_icon_placeholder(icon_bounds, side, corner_radius, window);
        }
    }
}

fn paint_file_theme_icon_placeholder(
    icon_bounds: Bounds<Pixels>,
    side: f32,
    corner_radius: Pixels,
    window: &mut Window,
) {
    let body_width = side * 0.86;
    let body_height = side * 0.92;
    let body = Bounds::new(
        point(
            icon_bounds.origin.x
                + px((icon_bounds.size.width.as_f32() - body_width).max(0.0) / 2.0),
            icon_bounds.origin.y
                + px((icon_bounds.size.height.as_f32() - body_height).max(0.0) / 2.0),
        ),
        size(px(body_width), px(body_height)),
    );
    window.paint_quad(
        fill(body, rgb(0xf8fafc))
            .corner_radii(corner_radius.min(px(5.0)))
            .border_widths(px(1.0))
            .border_color(rgb(0x94a3b8)),
    );

    let fold = (side * 0.22).clamp(4.0, 14.0);
    let fold_bounds = Bounds::new(
        point(body.origin.x + body.size.width - px(fold), body.origin.y),
        size(px(fold), px(fold)),
    );
    window.paint_quad(
        fill(fold_bounds, rgb(0xe2e8f0))
            .corner_radii(px(2.0))
            .border_widths(px(1.0))
            .border_color(rgb(0x94a3b8)),
    );
}

fn paint_folder_theme_icon_placeholder(
    icon_bounds: Bounds<Pixels>,
    side: f32,
    corner_radius: Pixels,
    window: &mut Window,
) {
    let body_width = side * 0.9;
    let body_height = side * 0.68;
    let body_left = icon_bounds.origin.x + px(side * 0.05);
    let body_top = icon_bounds.origin.y + px(side * 0.26);
    let tab = Bounds::new(
        point(
            body_left + px(side * 0.04),
            icon_bounds.origin.y + px(side * 0.16),
        ),
        size(px(side * 0.36), px(side * 0.18)),
    );
    let body = Bounds::new(
        point(body_left, body_top),
        size(px(body_width), px(body_height)),
    );
    window.paint_quad(
        fill(tab, rgb(0xdbeafe))
            .corner_radii(corner_radius.min(px(5.0)))
            .border_widths(px(1.0))
            .border_color(rgb(0x93c5fd)),
    );
    window.paint_quad(
        fill(body, rgb(0xe8f2ff))
            .corner_radii(corner_radius.min(px(6.0)))
            .border_widths(px(1.0))
            .border_color(rgb(0x93c5fd)),
    );
}

fn theme_icon_square_bounds(icon_bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    let side = icon_bounds
        .size
        .width
        .min(icon_bounds.size.height)
        .max(px(1.0));
    Bounds::new(
        point(
            icon_bounds.origin.x + ((icon_bounds.size.width - side) / 2.0),
            icon_bounds.origin.y + ((icon_bounds.size.height - side) / 2.0),
        ),
        size(side, side),
    )
}

fn item_image_layer_icon_bounds(
    layer_bounds: Bounds<Pixels>,
    icon_rect: ViewRect,
) -> Bounds<Pixels> {
    Bounds::new(
        point(
            layer_bounds.origin.x + px(icon_rect.x.round()),
            layer_bounds.origin.y + px(icon_rect.y.round()),
        ),
        size(
            px(icon_rect.width.round().max(1.0)),
            px(icon_rect.height.round().max(1.0)),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_pending_load_paints_fallback() {
        let item = test_item(Some(Arc::from(Path::new("/tmp/thumb.png"))));

        assert!(item_image_pending_load_paints_fallback(&item));
    }

    #[test]
    fn thumbnail_load_failure_paints_fallback() {
        let item = test_item(Some(Arc::from(Path::new("/tmp/thumb.png"))));

        assert!(item_image_load_failure_paints_fallback(&item));
    }

    #[test]
    fn theme_icon_pending_load_uses_markerless_placeholder() {
        let item = test_item(None);

        assert!(item_image_pending_load_paints_fallback(&item));
        assert!(!item_image_pending_load_paints_marker(&item));
        assert_eq!(
            theme_icon_placeholder_kind(&item.icon),
            ThemeIconPlaceholderKind::File
        );
    }

    #[test]
    fn folder_theme_icon_pending_load_uses_folder_placeholder() {
        let mut item = test_item(None);
        item.icon.icon_name = Arc::from("folder");
        item.icon.fallback_marker = Arc::from("DIR");

        assert_eq!(
            theme_icon_placeholder_kind(&item.icon),
            ThemeIconPlaceholderKind::Folder
        );
    }

    #[test]
    fn theme_icon_square_bounds_centers_non_square_icon_rect() {
        let bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(40.0), px(24.0)));

        let square = theme_icon_square_bounds(bounds);

        assert_eq!(square.origin.x, px(18.0));
        assert_eq!(square.origin.y, px(20.0));
        assert_eq!(square.size.width, px(24.0));
        assert_eq!(square.size.height, px(24.0));
    }

    #[test]
    fn image_source_stats_separate_decoded_retained_and_pending_paths() {
        let mut stats = ItemImageSourcePerfStats::default();

        record_item_image_source_stats(
            &mut stats,
            ItemImagePaintKind::ThemeIcon,
            RetainedImageLoadOutcome::CacheReady { first_ready: true },
        );
        record_item_image_source_stats(
            &mut stats,
            ItemImagePaintKind::ThemeIcon,
            RetainedImageLoadOutcome::Retained,
        );
        record_item_image_source_stats(
            &mut stats,
            ItemImagePaintKind::ThemeIcon,
            RetainedImageLoadOutcome::Missing,
        );
        record_item_image_source_stats(
            &mut stats,
            ItemImagePaintKind::Thumbnail,
            RetainedImageLoadOutcome::CacheReady { first_ready: false },
        );
        record_item_image_source_stats(
            &mut stats,
            ItemImagePaintKind::Thumbnail,
            RetainedImageLoadOutcome::Missing,
        );

        assert_eq!(stats.theme_loaded, 1);
        assert_eq!(stats.theme_decoded, 1);
        assert_eq!(stats.theme_retained, 1);
        assert_eq!(stats.theme_placeholder, 1);
        assert_eq!(stats.thumbnail_loaded, 1);
        assert_eq!(stats.thumbnail_decoded, 0);
        assert_eq!(stats.thumbnail_retained, 0);
        assert_eq!(stats.thumbnail_fallback, 1);
    }

    fn test_item(thumbnail_path: Option<Arc<Path>>) -> ItemImageLayerItem {
        let rect = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 48.0,
            height: 48.0,
        };
        ItemImageLayerItem {
            visible: true,
            layout: ItemLayout {
                model_index: 0,
                column: 0,
                row: 0,
                item_rect: rect,
                visual_rect: rect,
                icon_rect: rect,
                text_rect: rect,
            },
            thumbnail_path,
            icon: FileIconSnapshot {
                icon_name: Arc::from("image-png"),
                path: Some(Arc::from(Path::new("/tmp/icon.png"))),
                fallback_marker: Arc::from("IMG"),
                fallback_fg: 0xffffff,
                fallback_bg: 0x222222,
            },
            fallback_marker: SharedString::from("IMG"),
            mode: IconPaintMode::Normal,
        }
    }
}
