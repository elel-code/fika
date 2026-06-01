use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Manager,
    Chooser,
}

#[derive(Debug)]
pub struct Args {
    pub mode: Mode,
    pub start_dir: Option<PathBuf>,
    pub chooser_select_directories: bool,
    pub chooser_multiple: bool,
    pub chooser_title: Option<String>,
    pub chooser_accept_label: Option<String>,
    pub chooser_save_name: Option<String>,
    pub chooser_save_files: Vec<String>,
    pub chooser_filters: Vec<String>,
    pub chooser_filter_index: usize,
    pub chooser_return_filter: bool,
    pub chooser_choices: Vec<String>,
    pub chooser_return_choices: bool,
    pub chooser_parent_window: Option<String>,
}

impl Args {
    pub fn parse(args: impl Iterator<Item = String>) -> Self {
        let mut mode = Mode::Manager;
        let mut start_dir = None;
        let mut chooser_select_directories = false;
        let mut chooser_multiple = false;
        let mut chooser_title = None;
        let mut chooser_accept_label = None;
        let mut chooser_save_name = None;
        let mut chooser_save_files = Vec::new();
        let mut chooser_filters = Vec::new();
        let mut chooser_filter_index = 0usize;
        let mut chooser_return_filter = false;
        let mut chooser_choices = Vec::new();
        let mut chooser_return_choices = false;
        let mut chooser_parent_window = None;
        let mut pending_save_name = false;
        let mut pending_save_files = false;
        let mut pending_title = false;
        let mut pending_accept_label = false;
        let mut pending_filters = false;
        let mut pending_filter_index = false;
        let mut pending_choices = false;
        let mut pending_parent_window = false;

        for arg in args {
            if pending_save_name {
                chooser_save_name = Some(arg);
                pending_save_name = false;
                continue;
            }
            if pending_save_files {
                chooser_save_files = arg
                    .split('\n')
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .collect();
                chooser_select_directories = true;
                pending_save_files = false;
                continue;
            }
            if pending_title {
                chooser_title = (!arg.is_empty()).then_some(arg);
                pending_title = false;
                continue;
            }
            if pending_accept_label {
                chooser_accept_label = Some(arg);
                pending_accept_label = false;
                continue;
            }
            if pending_filters {
                chooser_filters = arg
                    .split('\n')
                    .filter(|filter| !filter.is_empty())
                    .map(str::to_string)
                    .collect();
                pending_filters = false;
                continue;
            }
            if pending_filter_index {
                chooser_filter_index = arg.parse().unwrap_or_default();
                pending_filter_index = false;
                continue;
            }
            if pending_choices {
                chooser_choices = arg
                    .split('\n')
                    .filter(|choice| !choice.is_empty())
                    .map(str::to_string)
                    .collect();
                pending_choices = false;
                continue;
            }
            if pending_parent_window {
                chooser_parent_window = (!arg.is_empty()).then_some(arg);
                pending_parent_window = false;
                continue;
            }

            match arg.as_str() {
                "--chooser" => mode = Mode::Chooser,
                "--chooser-directory" => {
                    mode = Mode::Chooser;
                    chooser_select_directories = true;
                }
                "--chooser-multiple" => {
                    mode = Mode::Chooser;
                    chooser_multiple = true;
                }
                "--chooser-save" => {
                    mode = Mode::Chooser;
                    pending_save_name = true;
                }
                "--chooser-save-files" => {
                    mode = Mode::Chooser;
                    pending_save_files = true;
                }
                "--chooser-title" => {
                    mode = Mode::Chooser;
                    pending_title = true;
                }
                "--chooser-accept-label" => {
                    mode = Mode::Chooser;
                    pending_accept_label = true;
                }
                "--chooser-filters" => {
                    mode = Mode::Chooser;
                    pending_filters = true;
                }
                "--chooser-filter-index" => {
                    mode = Mode::Chooser;
                    pending_filter_index = true;
                }
                "--chooser-return-filter" => {
                    mode = Mode::Chooser;
                    chooser_return_filter = true;
                }
                "--chooser-choices" => {
                    mode = Mode::Chooser;
                    pending_choices = true;
                }
                "--chooser-return-choices" => {
                    mode = Mode::Chooser;
                    chooser_return_choices = true;
                }
                "--chooser-parent-window" => {
                    mode = Mode::Chooser;
                    pending_parent_window = true;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                _ if start_dir.is_none() => start_dir = Some(PathBuf::from(arg)),
                _ => {}
            }
        }

        Self {
            mode,
            start_dir,
            chooser_select_directories,
            chooser_multiple,
            chooser_title,
            chooser_accept_label,
            chooser_save_name,
            chooser_save_files,
            chooser_filters,
            chooser_filter_index,
            chooser_return_filter,
            chooser_choices,
            chooser_return_choices,
            chooser_parent_window,
        }
    }
}

fn print_help() {
    println!(
        "Usage: fika [--chooser] [START_DIR]\n\n\
         Options:\n\
           --chooser                 Start in lightweight file chooser mode.\n\
           --chooser-directory       Choose a folder instead of a file.\n\
           --chooser-multiple        Allow returning multiple selected files.\n\
           --chooser-save NAME       Choose a save path with NAME prefilled.\n\
           --chooser-save-files LIST Choose a target folder for newline-separated file names.\n\
           --chooser-title TITLE     Use TITLE as the chooser window title.\n\
           --chooser-accept-label L  Use L for the chooser confirmation button.\n\
           --chooser-filters LIST    Use newline-separated label/pattern filter specs.\n\
           --chooser-filter-index N  Select initial chooser filter index.\n\
           --chooser-return-filter   Print selected filter metadata before paths.\n\
           --chooser-choices LIST    Use newline-separated portal choice specs.\n\
           --chooser-return-choices  Print selected choice metadata before paths.\n\
           --chooser-parent-window W Preserve portal parent window handle metadata.\n\
           -h, --help                Show this help."
    );
}

#[cfg(test)]
mod tests {
    use super::{Args, Mode};

    #[test]
    fn parses_chooser_parent_window() {
        let args = Args::parse(
            [
                "--chooser-title",
                "Pick a File",
                "--chooser-parent-window",
                "wayland:1_42",
                "--chooser",
                "/tmp",
            ]
            .into_iter()
            .map(str::to_string),
        );

        assert_eq!(args.mode, Mode::Chooser);
        assert_eq!(args.chooser_title.as_deref(), Some("Pick a File"));
        assert_eq!(args.chooser_parent_window.as_deref(), Some("wayland:1_42"));
        assert_eq!(args.start_dir.unwrap().to_string_lossy(), "/tmp");
    }

    #[test]
    fn ignores_empty_chooser_parent_window() {
        let args = Args::parse(
            ["--chooser-parent-window", ""]
                .into_iter()
                .map(str::to_string),
        );

        assert_eq!(args.mode, Mode::Chooser);
        assert_eq!(args.chooser_parent_window, None);
    }
}
