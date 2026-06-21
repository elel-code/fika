use std::error::Error;
use std::path::PathBuf;

use fika_core::{ViewMode, ViewPoint, ViewRect};

use super::metrics::{
    APP_TOOLBAR_HEIGHT, PANE_MARGIN, PLACES_ICON_SIZE, PLACES_PANEL_MARGIN_BOTTOM,
    PLACES_PANEL_MARGIN_X, PLACES_ROW_HEIGHT, PLACES_TITLE_HEIGHT, PLACES_TO_PANE_GAP,
    PLACES_WIDTH, TEXT_FONT_SIZE, TEXT_LINE_HEIGHT,
};
use super::pane::{PaneGeometry, SctkPane};
use super::quad::{QuadBatch, inset};
use super::text::TextBatch;

const TEXT_PRIMARY: [u8; 4] = [36, 41, 47, 255];
const TEXT_MUTED: [u8; 4] = [89, 99, 110, 255];

pub(crate) struct SctkScene {
    pane: SctkPane,
    places_visible: bool,
}

impl SctkScene {
    pub(crate) fn load(path: PathBuf, view_mode: ViewMode) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            pane: SctkPane::load(path, view_mode)?,
            places_visible: true,
        })
    }

    pub(crate) fn log_startup(&self) {
        eprintln!(
            "[fika-sctk] path={} view={} entries={} dirs={} files={}",
            self.path().display(),
            self.view_mode().as_str(),
            self.entry_count(),
            self.dir_count(),
            self.file_count()
        );
    }

    pub(crate) fn path(&self) -> &PathBuf {
        self.pane.path()
    }

    pub(crate) fn view_mode(&self) -> ViewMode {
        self.pane.view_mode()
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.pane.entry_count()
    }

    pub(crate) fn dir_count(&self) -> usize {
        self.pane.dir_count()
    }

    pub(crate) fn file_count(&self) -> usize {
        self.pane.file_count()
    }

    pub(crate) fn render_frame(&mut self, width: u32, height: u32) -> SceneFrame {
        let geometry = self.geometry(width, height);
        let window = ViewRect {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
        };
        let mut batch = QuadBatch::default();
        let mut text = TextBatch::default();
        self.push_app_chrome(&mut batch, &mut text, geometry, window, width, height);
        let pane_stats = self.pane.render(
            &mut batch,
            &mut text,
            geometry.pane_geometry(),
            window,
            width,
            height,
        );

        let quads = batch.len();
        SceneFrame {
            batch,
            text,
            quads,
            visible_items: pane_stats.visible_items,
            selected: pane_stats.selected,
            hover: pane_stats.hover,
            scroll_x: pane_stats.scroll_x,
            scroll_y: pane_stats.scroll_y,
        }
    }

    pub(crate) fn set_pointer(&mut self, point: ViewPoint, width: u32, height: u32) -> bool {
        let geometry = self.geometry(width, height);
        self.pane.set_pointer(point, geometry.pane_geometry())
    }

    pub(crate) fn clear_pointer(&mut self) -> bool {
        self.pane.clear_pointer()
    }

    pub(crate) fn press_primary(&mut self, point: ViewPoint, width: u32, height: u32) -> bool {
        let geometry = self.geometry(width, height);
        self.pane.press_primary(point, geometry.pane_geometry())
    }

    pub(crate) fn scroll(&mut self, delta_x: f32, delta_y: f32, width: u32, height: u32) -> bool {
        let geometry = self.geometry(width, height);
        self.pane.scroll(delta_x, delta_y, geometry.pane_geometry())
    }

    fn push_app_chrome(
        &self,
        batch: &mut QuadBatch,
        text: &mut TextBatch,
        geometry: SceneGeometry,
        window: ViewRect,
        width: u32,
        height: u32,
    ) {
        batch.push_rect(window, [0.91, 0.93, 0.95, 1.0], width, height);
        batch.push_rect(
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: width as f32,
                height: APP_TOOLBAR_HEIGHT,
            },
            [0.83, 0.86, 0.89, 1.0],
            width,
            height,
        );

        if let Some(places) = geometry.places {
            batch.push_clipped_rounded_rect(
                places,
                window,
                8.0,
                [0.96, 0.97, 0.98, 1.0],
                width,
                height,
            );
            batch.push_rect(
                ViewRect {
                    x: places.x + 10.0,
                    y: places.y + PLACES_TITLE_HEIGHT + 2.0,
                    width: places.width - 20.0,
                    height: 1.0,
                },
                [0.82, 0.84, 0.86, 1.0],
                width,
                height,
            );
            text.push(
                "Places",
                ViewRect {
                    x: places.x + 14.0,
                    y: places.y + 7.0,
                    width: places.width - 28.0,
                    height: TEXT_LINE_HEIGHT,
                },
                places,
                TEXT_FONT_SIZE,
                TEXT_LINE_HEIGHT,
                TEXT_MUTED,
            );
            self.push_places_rows(batch, text, places, width, height);
        }
    }

    fn push_places_rows(
        &self,
        batch: &mut QuadBatch,
        text: &mut TextBatch,
        places: ViewRect,
        width: u32,
        height: u32,
    ) {
        let clip = inset(places, 6.0);
        let rows = [
            ("Home", true),
            ("Desktop", false),
            ("Documents", false),
            ("Downloads", false),
            ("Trash", false),
            ("Root", self.path() == &PathBuf::from("/")),
        ];
        for (row, (label, active)) in rows.iter().enumerate() {
            let y = places.y + PLACES_TITLE_HEIGHT + 8.0 + row as f32 * PLACES_ROW_HEIGHT;
            let rect = ViewRect {
                x: places.x + 8.0,
                y,
                width: places.width - 16.0,
                height: PLACES_ROW_HEIGHT - 2.0,
            };
            if *active {
                batch.push_clipped_rounded_rect(
                    rect,
                    clip,
                    6.0,
                    [0.80, 0.88, 0.96, 1.0],
                    width,
                    height,
                );
            }
            let icon = ViewRect {
                x: rect.x + 8.0,
                y: rect.y + (rect.height - PLACES_ICON_SIZE) / 2.0,
                width: PLACES_ICON_SIZE,
                height: PLACES_ICON_SIZE,
            };
            batch.push_clipped_rounded_rect(
                icon,
                clip,
                5.0,
                [0.24, 0.45, 0.68, 1.0],
                width,
                height,
            );
            text.push_no_wrap(
                *label,
                ViewRect {
                    x: icon.right() + 9.0,
                    y: rect.y + (rect.height - TEXT_LINE_HEIGHT) / 2.0,
                    width: (rect.right() - icon.right() - 17.0).max(1.0),
                    height: TEXT_LINE_HEIGHT,
                },
                clip,
                TEXT_FONT_SIZE,
                TEXT_LINE_HEIGHT,
                if *active { TEXT_PRIMARY } else { TEXT_MUTED },
            );
        }
    }

    fn geometry(&self, width: u32, height: u32) -> SceneGeometry {
        let window_width = width as f32;
        let window_height = height as f32;
        let top = APP_TOOLBAR_HEIGHT + PANE_MARGIN;
        let bottom = PANE_MARGIN;
        let places = self.places_visible.then_some(ViewRect {
            x: PLACES_PANEL_MARGIN_X,
            y: top,
            width: PLACES_WIDTH,
            height: (window_height - top - PLACES_PANEL_MARGIN_BOTTOM).max(1.0),
        });
        let pane_x = places
            .map(|places| places.right() + PLACES_TO_PANE_GAP)
            .unwrap_or(PANE_MARGIN);
        let pane = ViewRect {
            x: pane_x,
            y: top,
            width: (window_width - pane_x - PANE_MARGIN).max(1.0),
            height: (window_height - top - bottom).max(1.0),
        };
        SceneGeometry {
            places,
            pane: PaneGeometry {
                pane,
                content: ViewRect {
                    x: pane.x,
                    y: pane.y + super::metrics::TOP_BAR_HEIGHT,
                    width: pane.width,
                    height: (pane.height
                        - super::metrics::TOP_BAR_HEIGHT
                        - super::metrics::STATUS_BAR_HEIGHT)
                        .max(1.0),
                },
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SceneGeometry {
    places: Option<ViewRect>,
    pane: PaneGeometry,
}

impl SceneGeometry {
    fn pane_geometry(self) -> PaneGeometry {
        self.pane
    }
}

pub(crate) struct SceneFrame {
    pub(crate) batch: QuadBatch,
    pub(crate) text: TextBatch,
    pub(crate) quads: usize,
    pub(crate) visible_items: usize,
    pub(crate) selected: Option<usize>,
    pub(crate) hover: Option<usize>,
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fika_core::{Entry, EntryData};

    use crate::fika_sctk::metrics::SCROLL_LINE_PX;

    use super::*;

    fn entry(name: &str, is_dir: bool) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len().min(u16::MAX as usize) as u16,
            target_path: None,
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: true,
            mime_type: None,
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir,
        })
    }

    fn scene(view_mode: ViewMode, count: usize) -> SctkScene {
        let entries = (0..count)
            .map(|index| entry(&format!("item-{index}"), index % 5 == 0))
            .collect::<Vec<_>>();
        SctkScene {
            pane: SctkPane::from_entries(PathBuf::from("/tmp"), view_mode, entries),
            places_visible: true,
        }
    }

    #[test]
    fn renders_all_view_modes_as_real_quad_frames() {
        for mode in [ViewMode::Icons, ViewMode::Compact, ViewMode::Details] {
            let mut scene = scene(mode, 80);
            let frame = scene.render_frame(900, 640);
            assert!(
                frame.quads > 20,
                "{mode:?} should paint chrome and file items"
            );
            assert!(
                frame.visible_items > 0,
                "{mode:?} should project visible items"
            );
            assert!(frame.text.len() > 0, "{mode:?} should paint real labels");
        }
    }

    #[test]
    fn pointer_hit_selects_projected_item() {
        let mut scene = scene(ViewMode::Icons, 20);
        let geometry = scene.geometry(900, 640).pane_geometry();
        let item = scene.pane.icons_layout(geometry.content).item(0).unwrap();
        let point = ViewPoint {
            x: geometry.content.x + item.visual_rect.x + 4.0,
            y: geometry.content.y + item.visual_rect.y + 4.0,
        };

        assert!(scene.set_pointer(point, 900, 640));
        assert_eq!(scene.pane.hover(), Some(0));
        assert!(scene.press_primary(point, 900, 640));
        assert_eq!(scene.pane.selected(), Some(0));
    }

    #[test]
    fn scroll_is_clamped_per_view_mode_axis() {
        let mut icons = scene(ViewMode::Icons, 300);
        assert!(icons.scroll(0.0, SCROLL_LINE_PX * 20.0, 900, 480));
        let frame = icons.render_frame(900, 480);
        assert!(frame.scroll_y > 0.0);
        assert_eq!(frame.scroll_x, 0.0);

        let mut compact = scene(ViewMode::Compact, 300);
        assert!(compact.scroll(0.0, SCROLL_LINE_PX * 20.0, 900, 480));
        let frame = compact.render_frame(900, 480);
        assert!(frame.scroll_x > 0.0);
        assert_eq!(frame.scroll_y, 0.0);
    }

    #[test]
    fn scene_geometry_keeps_pane_as_reusable_component_bounds() {
        let scene = scene(ViewMode::Icons, 1);
        let geometry = scene.geometry(900, 640);
        assert!(geometry.pane.pane.width > 0.0);
        assert!(geometry.pane.content.width > 0.0);
        assert!(geometry.pane.content.y > geometry.pane.pane.y);
        assert!(geometry.places.is_some());
    }
}
