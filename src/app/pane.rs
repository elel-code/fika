use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PaneNavigation {
    pub(crate) previous: PathBuf,
    pub(crate) target: PathBuf,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PaneHistory {
    back_stack: Vec<PathBuf>,
    forward_stack: Vec<PathBuf>,
}

impl PaneHistory {
    pub(crate) fn navigate_from(&mut self, previous: PathBuf, target: PathBuf) -> PaneNavigation {
        self.back_stack.push(previous.clone());
        self.forward_stack.clear();
        PaneNavigation { previous, target }
    }

    pub(crate) fn go_back_from(&mut self, previous: PathBuf) -> Option<PaneNavigation> {
        let target = self.back_stack.pop()?;
        self.forward_stack.push(previous.clone());
        Some(PaneNavigation { previous, target })
    }

    pub(crate) fn go_forward_from(&mut self, previous: PathBuf) -> Option<PaneNavigation> {
        let target = self.forward_stack.pop()?;
        self.back_stack.push(previous.clone());
        Some(PaneNavigation { previous, target })
    }

    pub(crate) fn prune_under(&mut self, mount_path: &Path) {
        self.back_stack.retain(|path| !path.starts_with(mount_path));
        self.forward_stack
            .retain(|path| !path.starts_with(mount_path));
    }

    pub(crate) fn back_len(&self) -> usize {
        self.back_stack.len()
    }

    pub(crate) fn forward_len(&self) -> usize {
        self.forward_stack.len()
    }

    #[cfg(test)]
    pub(crate) fn from_stacks(back_stack: Vec<PathBuf>, forward_stack: Vec<PathBuf>) -> Self {
        Self {
            back_stack,
            forward_stack,
        }
    }

    #[cfg(test)]
    pub(crate) fn back_paths(&self) -> &[PathBuf] {
        &self.back_stack
    }

    #[cfg(test)]
    pub(crate) fn forward_paths(&self) -> &[PathBuf] {
        &self.forward_stack
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_history_navigation_keeps_back_and_forward_independent() {
        let mut history = PaneHistory::default();
        let mut current = PathBuf::from("/home/yk");

        let nav = history.navigate_from(current.clone(), PathBuf::from("/tmp"));
        current = nav.target.clone();
        assert_eq!(nav.previous, PathBuf::from("/home/yk"));
        assert_eq!(nav.target, PathBuf::from("/tmp"));
        assert_eq!(current, PathBuf::from("/tmp"));
        assert_eq!(history.back_paths(), &[PathBuf::from("/home/yk")]);
        assert!(history.forward_paths().is_empty());

        let nav = history.go_back_from(current.clone()).unwrap();
        current = nav.target.clone();
        assert_eq!(nav.previous, PathBuf::from("/tmp"));
        assert_eq!(nav.target, PathBuf::from("/home/yk"));
        assert_eq!(current, PathBuf::from("/home/yk"));
        assert!(history.back_paths().is_empty());
        assert_eq!(history.forward_paths(), &[PathBuf::from("/tmp")]);

        let nav = history.go_forward_from(current.clone()).unwrap();
        current = nav.target.clone();
        assert_eq!(nav.previous, PathBuf::from("/home/yk"));
        assert_eq!(nav.target, PathBuf::from("/tmp"));
        assert_eq!(current, PathBuf::from("/tmp"));
        assert_eq!(history.back_paths(), &[PathBuf::from("/home/yk")]);
        assert!(history.forward_paths().is_empty());
    }

    #[test]
    fn pane_history_prunes_removed_mount_paths() {
        let mount_path = PathBuf::from("/run/media/yk/USB");
        let mut history = PaneHistory::from_stacks(
            vec![PathBuf::from("/tmp"), mount_path.join("old")],
            vec![
                mount_path.join("future"),
                PathBuf::from("/run/media/yk/USB-sibling"),
            ],
        );

        history.prune_under(&mount_path);

        assert_eq!(history.back_paths(), &[PathBuf::from("/tmp")]);
        assert_eq!(
            history.forward_paths(),
            &[PathBuf::from("/run/media/yk/USB-sibling")]
        );
    }
}
