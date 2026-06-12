pub(crate) fn sanitize_element_id(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "application".to_string()
    } else {
        sanitized
    }
}

pub(crate) fn application_marker(name: &str) -> String {
    let marker = name
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_ascii_uppercase();
    if marker.is_empty() {
        "APP".to_string()
    } else {
        marker
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_id_sanitizer_keeps_stable_ascii_widget_ids() {
        assert_eq!(
            sanitize_element_id("org.example.App.desktop"),
            "org-example-App-desktop"
        );
        assert_eq!(sanitize_element_id(""), "application");
    }

    #[test]
    fn application_marker_uses_first_two_alphanumeric_characters() {
        assert_eq!(application_marker("Okular"), "OK");
        assert_eq!(application_marker("1 Writer"), "1W");
        assert_eq!(application_marker("!!!"), "APP");
    }
}
