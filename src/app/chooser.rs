use crate::app::pane::PaneTarget;
use crate::app::state::{AppState, ChooserChoice, ChooserChoiceItem, ChooserFilter};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ChooserOutputMetadata {
    pub(crate) filter_index: Option<usize>,
    pub(crate) choices: Vec<(String, String)>,
}

pub(crate) fn parse_chooser_filter_spec(spec: &str) -> Option<ChooserFilter> {
    let (label, patterns) = spec.split_once('\t').unwrap_or((spec, ""));
    let label = label.trim();
    if label.is_empty() {
        return None;
    }
    let patterns = patterns
        .split(';')
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    Some(ChooserFilter {
        label: label.to_string(),
        patterns,
    })
}

pub(crate) fn parse_chooser_choice_spec(spec: &str) -> Option<ChooserChoice> {
    let parts = spec.split('\t').collect::<Vec<_>>();
    let [id, label, selected, items] = parts.as_slice() else {
        return None;
    };
    if id.is_empty() || label.is_empty() {
        return None;
    }

    let items = items
        .split(';')
        .filter_map(|item| {
            let (item_id, item_label) = item.split_once('=')?;
            if item_id.is_empty() || item_label.is_empty() {
                return None;
            }
            Some(ChooserChoiceItem {
                id: item_id.to_string(),
                label: item_label.to_string(),
            })
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        return None;
    }
    let selected_index = items
        .iter()
        .position(|item| item.id == *selected)
        .unwrap_or_default();

    Some(ChooserChoice {
        id: (*id).to_string(),
        label: (*label).to_string(),
        items,
        selected_index,
    })
}

pub(crate) fn set_chooser_choice_index(
    state: &mut AppState,
    choice_index: i32,
    option_index: i32,
) -> bool {
    let (Ok(choice_index), Ok(option_index)) =
        (usize::try_from(choice_index), usize::try_from(option_index))
    else {
        return false;
    };
    let Some(choice) = state.chooser_choices.get_mut(choice_index) else {
        return false;
    };
    if option_index >= choice.items.len() {
        return false;
    }
    choice.selected_index = option_index;
    true
}

pub(crate) fn chooser_output_metadata(state: &AppState) -> ChooserOutputMetadata {
    ChooserOutputMetadata {
        filter_index: if state.chooser_return_filter && !state.chooser_filters.is_empty() {
            Some(state.chooser_filter_index)
        } else {
            None
        },
        choices: if state.chooser_return_choices {
            state
                .chooser_choices
                .iter()
                .filter_map(|choice| {
                    choice
                        .items
                        .get(choice.selected_index)
                        .map(|item| (choice.id.clone(), item.id.clone()))
                })
                .collect()
        } else {
            Vec::new()
        },
    }
}

pub(crate) fn selected_directory_or_current(state: &AppState) -> PathBuf {
    let focused = state
        .panes
        .pane_for_target(PaneTarget::Focused)
        .unwrap_or(&state.panes.active());
    focused
        .selection
        .paths
        .first()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| focused.current_dir.clone())
}

pub(crate) fn safe_child_path(parent: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.as_bytes().contains(&0)
    {
        return None;
    }
    Some(parent.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chooser_filter_specs_parse_label_and_patterns() {
        assert_eq!(
            parse_chooser_filter_spec("Images\t*.png; *.jpg ;")
                .map(|filter| (filter.label, filter.patterns)),
            Some((
                "Images".to_string(),
                vec!["*.png".to_string(), "*.jpg".to_string()]
            ))
        );
        assert_eq!(
            parse_chooser_filter_spec("All Files").map(|filter| (filter.label, filter.patterns)),
            Some(("All Files".to_string(), Vec::new()))
        );
        assert!(parse_chooser_filter_spec("\t*.png").is_none());
    }

    #[test]
    fn chooser_choice_specs_parse_selected_item() {
        let choice =
            parse_chooser_choice_spec("encoding\tEncoding\tlatin1\tutf8=UTF-8;latin1=Latin-1")
                .unwrap();

        assert_eq!(choice.id, "encoding");
        assert_eq!(choice.label, "Encoding");
        assert_eq!(choice.selected_index, 1);
        assert_eq!(choice.items[1].id, "latin1");
    }

    #[test]
    fn chooser_choice_specs_reject_empty_or_invalid_specs() {
        assert!(parse_chooser_choice_spec("encoding\tEncoding\tutf8").is_none());
        assert!(parse_chooser_choice_spec("\tEncoding\tutf8\tutf8=UTF-8").is_none());
        assert!(parse_chooser_choice_spec("encoding\tEncoding\tutf8\tbroken").is_none());
    }

    #[test]
    fn chooser_choice_selection_and_output_metadata_are_stable() {
        let choice =
            parse_chooser_choice_spec("encoding\tEncoding\tlatin1\tutf8=UTF-8;latin1=Latin-1")
                .unwrap();
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.chooser_choices = vec![choice];
        state.chooser_return_choices = true;

        assert!(set_chooser_choice_index(&mut state, 0, 0));
        assert!(!set_chooser_choice_index(&mut state, 0, 9));
        assert!(!set_chooser_choice_index(&mut state, 9, 0));
        assert_eq!(
            chooser_output_metadata(&state),
            ChooserOutputMetadata {
                filter_index: None,
                choices: vec![("encoding".to_string(), "utf8".to_string())],
            }
        );
    }

    #[test]
    fn chooser_output_metadata_reports_filter_only_when_requested() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.chooser_filters = vec![parse_chooser_filter_spec("Images\t*.png").unwrap()];
        state.chooser_filter_index = 0;

        assert_eq!(chooser_output_metadata(&state).filter_index, None);
        state.chooser_return_filter = true;
        assert_eq!(chooser_output_metadata(&state).filter_index, Some(0));
    }

    #[test]
    fn safe_child_path_accepts_simple_names_only() {
        let parent = Path::new("/tmp");

        assert_eq!(
            safe_child_path(parent, " report.txt "),
            Some(PathBuf::from("/tmp/report.txt"))
        );
        for invalid in [
            "",
            ".",
            "..",
            "../report.txt",
            "nested/report.txt",
            "bad\\name",
        ] {
            assert!(safe_child_path(parent, invalid).is_none());
        }
        assert!(safe_child_path(parent, "bad\0name").is_none());
    }

    #[test]
    fn selected_directory_or_current_uses_focused_pane() {
        let mut state = AppState::new(PathBuf::from("/tmp/fika-left"), Vec::new());
        assert!(state.panes.open_inactive(PathBuf::from("/tmp/fika-right")));
        assert_eq!(
            selected_directory_or_current(&state),
            PathBuf::from("/tmp/fika-left")
        );

        assert!(state.panes.focus_slot(1));
        assert_eq!(
            selected_directory_or_current(&state),
            PathBuf::from("/tmp/fika-right")
        );
    }
}
