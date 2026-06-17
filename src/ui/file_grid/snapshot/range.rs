use std::ops::Range;

use super::{RawFileGridSnapshot, RawVisibleItemSnapshot};

impl RawFileGridSnapshot {
    pub(crate) fn visible_layout_range_and_count(&self) -> Option<(Range<usize>, usize)> {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => {
                layout_index_range_and_count(
                    items
                        .iter()
                        .filter(|item| item.visible)
                        .map(|item| item.layout.model_index),
                )
            }
            Self::Details { .. } => None,
        }
    }

    pub(crate) fn visible_work_range_and_count(&self) -> Option<(Range<usize>, usize)> {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => {
                raw_work_layout_range_and_count(items)
            }
            Self::Details { items, .. } => {
                layout_index_range_and_count(items.iter().map(|item| item.row_index))
            }
        }
    }
}

pub(super) fn layout_index_range_and_count(
    indexes: impl IntoIterator<Item = usize>,
) -> Option<(Range<usize>, usize)> {
    let mut indexes = indexes.into_iter();
    let first = indexes.next()?;
    let mut start = first;
    let mut end = first;
    let mut count = 1;
    for index in indexes {
        start = start.min(index);
        end = end.max(index);
        count += 1;
    }
    Some((start..end + 1, count))
}

fn raw_work_layout_range_and_count(
    items: &[RawVisibleItemSnapshot],
) -> Option<(Range<usize>, usize)> {
    layout_index_range_and_count(items.iter().map(|item| item.layout.model_index))
}
