use std::path::Path;
use std::sync::Arc;

use fika_core::{ItemLayout, PaneId, ViewRect};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Corners, Element, ElementId, Entity, FontWeight, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, ObjectFit, Pixels, RenderImage, Resource,
    RetainAllImageCache, SharedString, Style, StyleRefinement, Styled, TextAlign, TextRun,
    WeakEntity, Window, fill, point, px, rgb, size,
};

use crate::FikaApp;
use crate::ui::icons::FileIconSnapshot;

use super::ITEM_NAME_LINE_HEIGHT;
use super::paint_slots::ItemPaintSnapshot;
use super::renderer_policy::{item_uses_image_layer, item_uses_layer_visual_paint};
use super::text::static_paint_single_line_text;

pub(super) fn item_image_layer_view(
    pane_id: PaneId,
    items: &[ItemPaintSnapshot],
    width: f32,
    height: f32,
    app: WeakEntity<FikaApp>,
) -> Option<ItemImageLayerElement> {
    let items = item_image_layer_items(items);
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

pub(super) fn item_image_layer_items(items: &[ItemPaintSnapshot]) -> Vec<ItemImageLayerItem> {
    items
        .iter()
        .filter_map(|item| {
            let content = item.content.as_ref();
            if !item_uses_layer_visual_paint(content) || !item_uses_image_layer(content) {
                return None;
            }
            Some(ItemImageLayerItem {
                layout: item.layout,
                thumbnail_path: content.thumbnail_path.clone(),
                icon: content.icon.clone(),
                fallback_marker: content.fallback_marker.clone(),
            })
        })
        .collect()
}

pub(super) struct ItemImageLayerItem {
    layout: ItemLayout,
    thumbnail_path: Option<Arc<Path>>,
    icon: FileIconSnapshot,
    fallback_marker: SharedString,
}

pub(super) fn item_image_layer_item_source_path(item: &ItemImageLayerItem) -> Option<Arc<Path>> {
    item.thumbnail_path
        .clone()
        .or_else(|| item.icon.path.clone())
}

pub(super) fn item_image_load_failure_paints_fallback(item: &ItemImageLayerItem) -> bool {
    item.thumbnail_path.is_none()
}

pub(super) struct ItemImageLayerElement {
    pane_id: PaneId,
    app: WeakEntity<FikaApp>,
    items: Vec<ItemImageLayerItem>,
    style: StyleRefinement,
}

pub(super) struct ItemImagePaintState {
    icon_rect: ViewRect,
    image: Option<Arc<RenderImage>>,
    fallback: Option<ItemImageFallbackPaintState>,
}

pub(super) struct ItemImageFallbackPaintState {
    pub(super) marker_line: gpui::ShapedLine,
    pub(super) marker_line_height: Pixels,
    pub(super) fallback_bg: u32,
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
        let perf_started = crate::item_view_perf_enabled().then(std::time::Instant::now);
        window.with_element_state::<Entity<RetainAllImageCache>, _>(id, |cache, window| {
            let cache = cache.unwrap_or_else(|| RetainAllImageCache::new(cx));
            let states = self
                .items
                .iter()
                .filter_map(|item| item_image_layer_prepaint_item(item, &cache, window, cx))
                .collect::<Vec<_>>();
            if let Some(started) = perf_started {
                let elapsed = started.elapsed();
                let count = states.len();
                let _ = self.app.update(cx, |this, _cx| {
                    this.record_item_image_prepaint(self.pane_id, elapsed, count);
                });
            }
            (states, cache)
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
        let perf_started = crate::item_view_perf_enabled().then(std::time::Instant::now);
        let count = prepaint.len();
        request_layout.paint(bounds, window, cx, |window, cx| {
            for state in prepaint.iter() {
                item_image_layer_paint_item(bounds, state, window, cx);
            }
        });
        if let Some(started) = perf_started {
            let elapsed = started.elapsed();
            let _ = self.app.update(cx, |this, _cx| {
                this.record_item_image_paint(self.pane_id, elapsed, count);
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
    cache: &Entity<RetainAllImageCache>,
    window: &mut Window,
    cx: &mut App,
) -> Option<ItemImagePaintState> {
    let source_path = item_image_layer_item_source_path(item)?;
    let resource = Resource::Path(source_path);
    let load_result = cache.update(cx, |cache, cx| cache.load(&resource, window, cx));
    let (image, fallback) = match load_result {
        Some(Ok(image)) => (Some(image), None),
        Some(Err(_)) if item_image_load_failure_paints_fallback(item) => {
            (None, Some(item_image_fallback_prepaint(item, window)))
        }
        _ => (None, None),
    };
    Some(ItemImagePaintState {
        icon_rect: item.layout.icon_rect,
        image,
        fallback,
    })
}

fn item_image_fallback_prepaint(
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
        marker_line: window
            .text_system()
            .shape_line(marker, marker_font_size, &[marker_run], None),
        marker_line_height: px(item
            .layout
            .icon_rect
            .height
            .min(ITEM_NAME_LINE_HEIGHT)
            .max(1.0)),
        fallback_bg: item.icon.fallback_bg,
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
        let image_bounds = ObjectFit::Contain.get_bounds(icon_bounds, image_size);
        window
            .paint_image(image_bounds, Corners::all(px(6.0)), image.clone(), 0, false)
            .ok();
        return;
    }

    if let Some(fallback) = state.fallback.as_ref() {
        window.paint_quad(fill(icon_bounds, rgb(fallback.fallback_bg)).corner_radii(px(6.0)));
        let marker_origin = point(
            icon_bounds.origin.x,
            icon_bounds.origin.y
                + ((icon_bounds.size.height - fallback.marker_line_height).max(px(0.0)) / 2.0),
        );
        fallback
            .marker_line
            .paint(
                marker_origin,
                fallback.marker_line_height,
                TextAlign::Center,
                Some(icon_bounds.size.width),
                window,
                cx,
            )
            .ok();
    }
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
