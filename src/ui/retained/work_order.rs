use std::ops::Range;

const DOLPHIN_READ_AHEAD_PAGES: usize = 5;
const DOLPHIN_RESOLVE_ALL_ITEMS_LIMIT: usize = 500;

pub(crate) fn dolphin_read_ahead_indexes(
    visible_indexes: Range<usize>,
    item_count: usize,
    maximum_visible_items: usize,
) -> DolphinReadAheadIndexes {
    if item_count == 0 || visible_indexes.is_empty() {
        return DolphinReadAheadIndexes::empty();
    }

    let visible_start = visible_indexes.start.min(item_count);
    let visible_end = visible_indexes.end.min(item_count).max(visible_start);
    if visible_start >= visible_end {
        return DolphinReadAheadIndexes::empty();
    }

    let maximum_visible_items = maximum_visible_items.max(1);
    let read_ahead_items =
        (DOLPHIN_READ_AHEAD_PAGES * maximum_visible_items).min(DOLPHIN_RESOLVE_ALL_ITEMS_LIMIT / 2);
    let last_visible = visible_end - 1;
    let end_extended = (last_visible + read_ahead_items).min(item_count - 1);
    let begin_extended = visible_start.saturating_sub(read_ahead_items);

    let after_visible = visible_end..end_extended + 1;
    let before_visible = (begin_extended..visible_start).rev();
    let last_page_start = (end_extended + 1).max(item_count.saturating_sub(maximum_visible_items));
    let last_page = last_page_start..item_count;
    let first_page_end = begin_extended.min(maximum_visible_items);
    let first_page = 0..first_page_end;

    let initial_len =
        after_visible.len() + before_visible.len() + last_page.len() + first_page.len();
    let remaining = DOLPHIN_RESOLVE_ALL_ITEMS_LIMIT.saturating_sub(initial_len);
    let rest_after_visible = (end_extended + 1)..last_page_start;
    let rest_after_len = rest_after_visible.len().min(remaining);
    let rest_before_visible = (first_page_end..begin_extended)
        .rev()
        .take(remaining.saturating_sub(rest_after_len));

    DolphinReadAheadIndexes {
        phase: 0,
        after_visible,
        before_visible,
        last_page,
        first_page,
        rest_after_visible: rest_after_visible.take(remaining),
        rest_before_visible,
    }
}

pub(crate) struct DolphinReadAheadIndexes {
    phase: u8,
    after_visible: Range<usize>,
    before_visible: std::iter::Rev<Range<usize>>,
    last_page: Range<usize>,
    first_page: Range<usize>,
    rest_after_visible: std::iter::Take<Range<usize>>,
    rest_before_visible: std::iter::Take<std::iter::Rev<Range<usize>>>,
}

impl DolphinReadAheadIndexes {
    fn empty() -> Self {
        Self {
            phase: 0,
            after_visible: 0..0,
            before_visible: (0..0).rev(),
            last_page: 0..0,
            first_page: 0..0,
            rest_after_visible: (0..0).take(0),
            rest_before_visible: (0..0).rev().take(0),
        }
    }
}

impl Iterator for DolphinReadAheadIndexes {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let next = match self.phase {
                0 => self.after_visible.next(),
                1 => self.before_visible.next(),
                2 => self.last_page.next(),
                3 => self.first_page.next(),
                4 => self.rest_after_visible.next(),
                5 => self.rest_before_visible.next(),
                _ => return None,
            };
            if next.is_some() {
                return next;
            }
            self.phase += 1;
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.after_visible.len()
            + self.before_visible.len()
            + self.last_page.len()
            + self.first_page.len()
            + self.rest_after_visible.len()
            + self.rest_before_visible.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for DolphinReadAheadIndexes {}

pub(crate) fn dolphin_visible_work_indexes(
    visible_indexes: Range<usize>,
    item_count: usize,
    maximum_visible_items: usize,
) -> Vec<usize> {
    if item_count == 0 || visible_indexes.is_empty() {
        return Vec::new();
    }

    let visible_start = visible_indexes.start.min(item_count);
    let visible_end = visible_indexes.end.min(item_count).max(visible_start);
    if visible_start >= visible_end {
        return Vec::new();
    }

    let maximum_visible_items = maximum_visible_items.max(1);
    let mut result = (visible_start..visible_end).collect::<Vec<_>>();
    let read_ahead_items =
        (DOLPHIN_READ_AHEAD_PAGES * maximum_visible_items).min(DOLPHIN_RESOLVE_ALL_ITEMS_LIMIT / 2);
    let last_visible = visible_end - 1;
    let end_extended = (last_visible + read_ahead_items).min(item_count - 1);
    result.extend(visible_end..end_extended + 1);

    let begin_extended = visible_start.saturating_sub(read_ahead_items);
    result.extend((begin_extended..visible_start).rev());

    let begin_last_page = (end_extended + 1).max(item_count.saturating_sub(maximum_visible_items));
    result.extend(begin_last_page..item_count);

    let end_first_page = begin_extended.min(maximum_visible_items);
    result.extend(0..end_first_page);

    let mut remaining = DOLPHIN_RESOLVE_ALL_ITEMS_LIMIT.saturating_sub(result.len());
    let rest_after_start = end_extended + 1;
    let rest_after_end = begin_last_page;
    let rest_after_len = rest_after_end
        .saturating_sub(rest_after_start)
        .min(remaining);
    result.extend((rest_after_start..rest_after_end).take(rest_after_len));
    remaining = remaining.saturating_sub(rest_after_len);

    result.extend((end_first_page..begin_extended).rev().take(remaining));
    result
}

pub(crate) fn visit_visible_work_items_by_index<T, IsVisible, Visit>(
    items: &[T],
    mut is_visible: IsVisible,
    mut visit: Visit,
) where
    IsVisible: FnMut(&T) -> bool,
    Visit: FnMut(&T) -> bool,
{
    for item in items.iter().filter(|item| is_visible(item)) {
        if !visit(item) {
            return;
        }
    }
}

pub(crate) fn visit_dolphin_visible_work_files_first<T, IsVisible, ModelIndex, IsDir, Visit>(
    items: &[T],
    visible_range: Option<Range<usize>>,
    mut is_visible: IsVisible,
    mut model_index: ModelIndex,
    mut is_dir: IsDir,
    mut visit: Visit,
) where
    IsVisible: FnMut(&T) -> bool,
    ModelIndex: FnMut(&T) -> usize,
    IsDir: FnMut(&T) -> bool,
    Visit: FnMut(&T) -> bool,
{
    for item in items
        .iter()
        .filter(|item| is_visible(item) && !is_dir(item))
    {
        if !visit(item) {
            return;
        }
    }

    for item in items.iter().filter(|item| is_visible(item) && is_dir(item)) {
        if !visit(item) {
            return;
        }
    }

    if let Some(visible_range) = visible_range {
        for item in items
            .iter()
            .filter(|item| !is_visible(item) && model_index(item) >= visible_range.end)
        {
            if !visit(item) {
                return;
            }
        }

        for item in items
            .iter()
            .rev()
            .filter(|item| !is_visible(item) && model_index(item) < visible_range.start)
        {
            if !visit(item) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    struct WorkItem {
        index: usize,
        visible: bool,
        dir: bool,
    }

    #[test]
    fn dolphin_read_ahead_indexes_follow_roles_updater_order() {
        assert_eq!(
            dolphin_read_ahead_indexes(10..20, 100, 10).collect::<Vec<_>>(),
            (20..=69)
                .chain((0..10).rev())
                .chain(90..100)
                .chain(70..90)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            dolphin_read_ahead_indexes(0..2, 5, 2).collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        assert_eq!(
            dolphin_read_ahead_indexes(3..5, 5, 2).collect::<Vec<_>>(),
            vec![2, 1, 0]
        );
        assert_eq!(dolphin_read_ahead_indexes(0..0, 10, 2).len(), 0);
        assert_eq!(dolphin_read_ahead_indexes(0..2, 0, 2).len(), 0);
    }

    #[test]
    fn dolphin_visible_work_indexes_include_visible_then_read_ahead() {
        assert_eq!(
            dolphin_visible_work_indexes(10..20, 100, 10),
            (10..20)
                .chain(20..=69)
                .chain((0..10).rev())
                .chain(90..100)
                .chain(70..90)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            dolphin_visible_work_indexes(0..2, 5, 2),
            vec![0, 1, 2, 3, 4]
        );
        assert!(dolphin_visible_work_indexes(0..0, 10, 2).is_empty());
    }

    #[test]
    fn visible_work_files_first_matches_dolphin_hot_order() {
        let items = [
            WorkItem {
                index: 0,
                visible: false,
                dir: false,
            },
            WorkItem {
                index: 1,
                visible: true,
                dir: false,
            },
            WorkItem {
                index: 2,
                visible: true,
                dir: true,
            },
            WorkItem {
                index: 3,
                visible: true,
                dir: false,
            },
            WorkItem {
                index: 4,
                visible: false,
                dir: false,
            },
        ];
        let mut indexes = Vec::new();
        visit_dolphin_visible_work_files_first(
            &items,
            Some(1..4),
            |item| item.visible,
            |item| item.index,
            |item| item.dir,
            |item| {
                indexes.push(item.index);
                true
            },
        );
        assert_eq!(indexes, vec![1, 3, 2, 4, 0]);
    }
}
