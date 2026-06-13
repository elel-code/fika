use fika_core::MimeApplication;
use std::collections::HashMap;

pub(crate) fn dedup_application_chooser_applications(
    applications: Vec<MimeApplication>,
) -> Vec<MimeApplication> {
    let mut key_indexes: HashMap<String, usize> = HashMap::new();
    let mut deduped: Vec<MimeApplication> = Vec::with_capacity(applications.len());

    for app in applications {
        let keys = application_keys(&app);
        if let Some(index) = keys.iter().find_map(|key| key_indexes.get(key).copied()) {
            if app.is_default {
                deduped[index].is_default = true;
            }
            continue;
        }

        let index = deduped.len();
        for key in keys {
            key_indexes.entry(key).or_insert(index);
        }
        deduped.push(app);
    }

    deduped
}

pub(crate) fn application_chooser_filtered_applications(
    applications: &[MimeApplication],
    query: &str,
) -> Vec<MimeApplication> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return applications.to_vec();
    }

    applications
        .iter()
        .filter(|app| application_matches_terms(app, &terms))
        .cloned()
        .collect()
}

fn application_matches_terms(app: &MimeApplication, terms: &[String]) -> bool {
    let haystack = format!(
        "{}\n{}\n{}\n{}",
        app.name,
        app.id,
        app.exec,
        app.desktop_file.display()
    )
    .to_ascii_lowercase();
    terms.iter().all(|term| haystack.contains(term))
}

fn application_keys(app: &MimeApplication) -> Vec<String> {
    let mut keys = Vec::new();
    push_key(&mut keys, "id", &app.id);
    push_key(&mut keys, "desktop", &app.desktop_file.to_string_lossy());
    let name_exec = format!("{}|{}", app.name.trim(), app.exec.trim());
    push_key(&mut keys, "name-exec", &name_exec);
    keys
}

fn push_key(keys: &mut Vec<String>, prefix: &str, value: &str) {
    let normalized = normalize_token(value);
    if !normalized.is_empty() {
        keys.push(format!("{prefix}:{normalized}"));
    }
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(normalize_token)
        .filter(|term| !term.is_empty())
        .collect()
}

fn normalize_token(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn app(id: &str, name: &str, exec: &str, desktop_file: &str) -> MimeApplication {
        MimeApplication {
            id: id.to_string(),
            desktop_file: PathBuf::from(desktop_file),
            name: name.to_string(),
            exec: exec.to_string(),
            icon: None,
            is_default: false,
        }
    }

    #[test]
    fn dedup_preserves_order_and_merges_default_marker() {
        let mut duplicate_default = app(
            "viewer.desktop",
            "Viewer",
            "viewer %f",
            "/usr/share/applications/viewer.desktop",
        );
        duplicate_default.is_default = true;

        let applications = dedup_application_chooser_applications(vec![
            app(
                "viewer.desktop",
                "Viewer",
                "viewer %f",
                "/home/me/.local/share/applications/viewer.desktop",
            ),
            app(
                "writer.desktop",
                "Writer",
                "writer %f",
                "/usr/share/applications/writer.desktop",
            ),
            duplicate_default,
            app(
                "viewer-copy.desktop",
                "Viewer",
                "viewer %f",
                "/opt/apps/viewer-copy.desktop",
            ),
        ]);

        assert_eq!(
            applications
                .iter()
                .map(|app| (app.id.as_str(), app.is_default))
                .collect::<Vec<_>>(),
            vec![("viewer.desktop", true), ("writer.desktop", false)]
        );
    }

    #[test]
    fn filter_matches_name_id_exec_and_desktop_path_terms() {
        let applications = vec![
            app(
                "org.kde.kate.desktop",
                "Kate",
                "kate %U",
                "/usr/share/applications/org.kde.kate.desktop",
            ),
            app(
                "org.gnome.Nautilus.desktop",
                "Files",
                "nautilus %U",
                "/usr/share/applications/org.gnome.Nautilus.desktop",
            ),
        ];

        assert_eq!(
            application_chooser_filtered_applications(&applications, "kde kate")
                .iter()
                .map(|app| app.id.as_str())
                .collect::<Vec<_>>(),
            vec!["org.kde.kate.desktop"]
        );
        assert_eq!(
            application_chooser_filtered_applications(&applications, "nautilus")
                .iter()
                .map(|app| app.id.as_str())
                .collect::<Vec<_>>(),
            vec!["org.gnome.Nautilus.desktop"]
        );
        assert!(application_chooser_filtered_applications(&applications, "missing").is_empty());
    }
}
