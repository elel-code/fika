use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use fika_core::{DesktopLaunchPlan, MimeApplication, MimeApplicationCache};

use crate::shell::metrics::OPEN_WITH_CHOOSER_MAX_ROWS;
use crate::shell::shortcuts::OpenWithCommand;

#[path = "open_with/geometry.rs"]
pub(crate) mod geometry;
#[path = "open_with/paint.rs"]
pub(crate) mod paint;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellOpenWithChooser {
    pub(crate) path: PathBuf,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) applications: Vec<MimeApplication>,
    pub(crate) query: String,
    pub(crate) selected_index: usize,
    pub(crate) scroll_row: usize,
    pub(crate) set_as_default: bool,
    pub(crate) error: Option<String>,
}

impl ShellOpenWithChooser {
    pub(crate) fn new(
        path: PathBuf,
        mime_type: Option<Arc<str>>,
        applications: Vec<MimeApplication>,
    ) -> Self {
        let mut chooser = Self {
            path,
            mime_type,
            selected_index: applications
                .iter()
                .position(|application| application.is_default)
                .unwrap_or(0),
            applications,
            query: String::new(),
            scroll_row: 0,
            set_as_default: false,
            error: None,
        };
        chooser.ensure_selected_visible();
        chooser
    }

    pub(crate) fn filtered_indexes(&self) -> Vec<usize> {
        open_with_filtered_application_indexes(&self.applications, &self.query)
    }

    pub(crate) fn filtered_count(&self) -> usize {
        self.filtered_indexes().len()
    }

    pub(crate) fn visible_filtered_indexes(&self) -> Vec<usize> {
        let indexes = self.filtered_indexes();
        indexes
            .into_iter()
            .skip(self.scroll_row)
            .take(OPEN_WITH_CHOOSER_MAX_ROWS)
            .collect()
    }

    pub(crate) fn selected_application(&self) -> Option<&MimeApplication> {
        let indexes = self.filtered_indexes();
        let selected = self.selected_index.min(indexes.len().saturating_sub(1));
        let app_index = *indexes.get(selected)?;
        self.applications.get(app_index)
    }

    pub(crate) fn apply_command(&mut self, command: OpenWithCommand) -> bool {
        let old = self.clone();
        match command {
            OpenWithCommand::Insert(value) => {
                self.query.push_str(&value);
                self.selected_index = 0;
                self.scroll_row = 0;
                self.error = None;
            }
            OpenWithCommand::Backspace => {
                self.query.pop();
                self.selected_index = 0;
                self.scroll_row = 0;
                self.error = None;
            }
            OpenWithCommand::Cancel => return false,
            OpenWithCommand::MoveUp => self.move_selection(-1),
            OpenWithCommand::MoveDown => self.move_selection(1),
            OpenWithCommand::Commit | OpenWithCommand::Ignore => return false,
        }
        self.ensure_selected_visible();
        old != *self
    }

    pub(crate) fn select_filtered_row(&mut self, row: usize) -> bool {
        let count = self.filtered_count();
        if count == 0 {
            return false;
        }
        let old_selected = self.selected_index;
        let old_scroll = self.scroll_row;
        self.selected_index = row.min(count - 1);
        self.error = None;
        self.ensure_selected_visible();
        old_selected != self.selected_index || old_scroll != self.scroll_row
    }

    pub(crate) fn toggle_set_as_default(&mut self) -> bool {
        if self.mime_type.is_none() {
            return false;
        }
        self.set_as_default = !self.set_as_default;
        self.error = None;
        true
    }

    pub(crate) fn scroll_rows(&mut self, delta: isize) -> bool {
        let count = self.filtered_count();
        if count <= OPEN_WITH_CHOOSER_MAX_ROWS {
            return false;
        }
        let old_scroll = self.scroll_row;
        let max_scroll = count.saturating_sub(OPEN_WITH_CHOOSER_MAX_ROWS);
        self.scroll_row = if delta < 0 {
            self.scroll_row.saturating_sub(delta.unsigned_abs())
        } else {
            (self.scroll_row + delta as usize).min(max_scroll)
        };
        old_scroll != self.scroll_row
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.filtered_count();
        if count == 0 {
            self.selected_index = 0;
            self.scroll_row = 0;
            return;
        }
        let current = self.selected_index.min(count - 1);
        self.selected_index = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            (current + delta as usize).min(count - 1)
        };
    }

    fn ensure_selected_visible(&mut self) {
        let count = self.filtered_count();
        if count == 0 {
            self.selected_index = 0;
            self.scroll_row = 0;
            return;
        }
        self.selected_index = self.selected_index.min(count - 1);
        if self.selected_index < self.scroll_row {
            self.scroll_row = self.selected_index;
        } else if self.selected_index >= self.scroll_row + OPEN_WITH_CHOOSER_MAX_ROWS {
            self.scroll_row = self.selected_index + 1 - OPEN_WITH_CHOOSER_MAX_ROWS;
        }
        self.scroll_row = self
            .scroll_row
            .min(count.saturating_sub(OPEN_WITH_CHOOSER_MAX_ROWS));
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OpenWithLaunchRequest {
    pub(crate) path: PathBuf,
    pub(crate) app_name: String,
    pub(crate) default_update: Option<OpenWithDefaultUpdate>,
    pub(crate) plan: DesktopLaunchPlan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OpenWithDefaultUpdate {
    pub(crate) mime_type: String,
    pub(crate) desktop_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServiceMenuLaunchRequest {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) app_name: String,
    pub(crate) plan: DesktopLaunchPlan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenWithChooserClick {
    Outside,
    Inside,
    Cancel,
    Open,
    ToggleDefault,
    Row(usize),
}

pub(crate) fn open_with_applications_for_mime(
    cache: &MimeApplicationCache,
    mime: Option<&str>,
) -> Vec<MimeApplication> {
    let mut applications = Vec::new();
    let mut seen = BTreeSet::new();
    let associated = mime
        .map(|mime| cache.applications_for_mime(mime))
        .unwrap_or_default();
    for application in associated.into_iter().chain(cache.all_applications()) {
        if seen.insert(application.id.clone()) {
            applications.push(application);
        }
    }
    applications
}

pub(crate) fn open_with_filtered_application_indexes(
    applications: &[MimeApplication],
    query: &str,
) -> Vec<usize> {
    let terms = query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .collect::<Vec<_>>();
    applications
        .iter()
        .enumerate()
        .filter(|(_, application)| open_with_application_matches_terms(application, &terms))
        .map(|(index, _)| index)
        .collect()
}

fn open_with_application_matches_terms(application: &MimeApplication, terms: &[String]) -> bool {
    if terms.is_empty() {
        return true;
    }
    let haystacks = [
        application.name.to_ascii_lowercase(),
        application.id.to_ascii_lowercase(),
        application.exec.to_ascii_lowercase(),
        application
            .desktop_file
            .display()
            .to_string()
            .to_ascii_lowercase(),
        application
            .icon
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    ];
    terms.iter().all(|term| {
        haystacks
            .iter()
            .any(|haystack| haystack.contains(term.as_str()))
    })
}
