#[derive(Clone, Debug)]
pub(crate) struct ChooserState {
    pub(crate) directories: bool,
    pub(crate) multiple: bool,
    pub(crate) title: String,
    pub(crate) accept_label: String,
    pub(crate) filter_index: usize,
    pub(crate) return_filter: bool,
    pub(crate) choices: Vec<String>,
    pub(crate) return_choices: bool,
}

pub(crate) fn selected_choice_rows(specs: &[String]) -> Vec<String> {
    specs
        .iter()
        .filter_map(|spec| {
            let mut parts = spec.split('\t');
            let id = parts.next()?;
            let _label = parts.next()?;
            let default = parts.next().unwrap_or_default();
            let options = parts.next().unwrap_or_default();
            let selected = if default.is_empty() {
                options
                    .split(';')
                    .next()
                    .and_then(|option| option.split_once('=').map(|(value, _)| value))
                    .unwrap_or_default()
            } else {
                default
            };
            (!id.is_empty() && !selected.is_empty())
                .then(|| format!("FIKA_CHOOSER_CHOICE\t{id}\t{selected}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::selected_choice_rows;

    #[test]
    fn chooser_choice_defaults_are_returned_without_ui_state() {
        let rows = selected_choice_rows(&[
            "encoding\tEncoding\tutf8\tutf8=UTF-8;latin1=Latin-1".to_string()
        ]);
        assert_eq!(rows, vec!["FIKA_CHOOSER_CHOICE\tencoding\tutf8"]);
    }

    #[test]
    fn chooser_choice_falls_back_to_first_option() {
        let rows = selected_choice_rows(&["quality\tQuality\t\tlow=Low;high=High".to_string()]);
        assert_eq!(rows, vec!["FIKA_CHOOSER_CHOICE\tquality\tlow"]);
    }
}
