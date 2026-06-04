use super::paths::home_dir;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ServiceMenuPolicyMode {
    #[default]
    AllExceptDisabled,
    OnlyEnabled,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ServiceMenuPolicy {
    pub(crate) mode: ServiceMenuPolicyMode,
    pub(crate) enabled_ids: HashSet<String>,
    pub(crate) disabled_ids: HashSet<String>,
}

impl ServiceMenuPolicy {
    pub(crate) fn is_enabled(&self, id: &str) -> bool {
        match self.mode {
            ServiceMenuPolicyMode::AllExceptDisabled => !self.disabled_ids.contains(id),
            ServiceMenuPolicyMode::OnlyEnabled => self.enabled_ids.contains(id),
        }
    }

    pub(crate) fn set_enabled(&mut self, id: &str, enabled: bool) {
        match self.mode {
            ServiceMenuPolicyMode::AllExceptDisabled => {
                if enabled {
                    self.disabled_ids.remove(id);
                } else {
                    self.disabled_ids.insert(id.to_string());
                }
            }
            ServiceMenuPolicyMode::OnlyEnabled => {
                if enabled {
                    self.enabled_ids.insert(id.to_string());
                } else {
                    self.enabled_ids.remove(id);
                }
            }
        }
    }
}

pub(crate) fn load_service_menu_policy() -> ServiceMenuPolicy {
    load_service_menu_policy_from(&service_menu_policy_path())
}

pub(crate) fn save_service_menu_policy(policy: &ServiceMenuPolicy) -> Result<(), String> {
    save_service_menu_policy_to(policy, &service_menu_policy_path())
}

pub(crate) fn load_service_menu_policy_from(path: &Path) -> ServiceMenuPolicy {
    let Ok(contents) = fs::read_to_string(path) else {
        return ServiceMenuPolicy::default();
    };

    let mut policy = ServiceMenuPolicy::default();
    for line in contents.lines() {
        let Some((key, value)) = line.split_once('\t') else {
            continue;
        };
        match key {
            "mode" => match value {
                "all_except_disabled" => policy.mode = ServiceMenuPolicyMode::AllExceptDisabled,
                "only_enabled" => policy.mode = ServiceMenuPolicyMode::OnlyEnabled,
                _ => {}
            },
            "enabled" => {
                if let Some(id) = unescape_value(value) {
                    policy.enabled_ids.insert(id);
                }
            }
            "disabled" => {
                if let Some(id) = unescape_value(value) {
                    policy.disabled_ids.insert(id);
                }
            }
            _ => {}
        }
    }
    policy
}

pub(crate) fn save_service_menu_policy_to(
    policy: &ServiceMenuPolicy,
    path: &Path,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let mut lines = Vec::new();
    lines.push(format!(
        "mode\t{}",
        match policy.mode {
            ServiceMenuPolicyMode::AllExceptDisabled => "all_except_disabled",
            ServiceMenuPolicyMode::OnlyEnabled => "only_enabled",
        }
    ));
    for id in sorted_ids(&policy.enabled_ids) {
        lines.push(format!("enabled\t{}", escape_value(id)));
    }
    for id in sorted_ids(&policy.disabled_ids) {
        lines.push(format!("disabled\t{}", escape_value(id)));
    }

    fs::write(path, lines.join("\n")).map_err(|err| err.to_string())
}

fn service_menu_policy_path() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("fika")
        .join("service-menu-policy.tsv")
}

fn sorted_ids(ids: &HashSet<String>) -> Vec<&str> {
    let mut sorted = ids.iter().map(String::as_str).collect::<Vec<_>>();
    sorted.sort_unstable();
    sorted
}

fn escape_value(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn unescape_value(value: &str) -> Option<String> {
    let mut unescaped = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            unescaped.push(ch);
            continue;
        }
        match chars.next()? {
            '\\' => unescaped.push('\\'),
            'n' => unescaped.push('\n'),
            't' => unescaped.push('\t'),
            _ => return None,
        }
    }
    Some(unescaped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_policy_allows_actions_until_disabled() {
        let mut policy = ServiceMenuPolicy::default();

        assert!(policy.is_enabled("/tmp/a.desktop:open"));
        policy.set_enabled("/tmp/a.desktop:open", false);
        assert!(!policy.is_enabled("/tmp/a.desktop:open"));
        policy.set_enabled("/tmp/a.desktop:open", true);
        assert!(policy.is_enabled("/tmp/a.desktop:open"));
    }

    #[test]
    fn only_enabled_policy_hides_unknown_actions() {
        let mut policy = ServiceMenuPolicy {
            mode: ServiceMenuPolicyMode::OnlyEnabled,
            ..ServiceMenuPolicy::default()
        };

        assert!(!policy.is_enabled("/tmp/a.desktop:open"));
        policy.set_enabled("/tmp/a.desktop:open", true);
        assert!(policy.is_enabled("/tmp/a.desktop:open"));
        policy.set_enabled("/tmp/a.desktop:open", false);
        assert!(!policy.is_enabled("/tmp/a.desktop:open"));
    }

    #[test]
    fn policy_roundtrip_preserves_escaped_ids() {
        let path = test_path("roundtrip").join("service-menu-policy.tsv");
        let mut policy = ServiceMenuPolicy {
            mode: ServiceMenuPolicyMode::OnlyEnabled,
            ..ServiceMenuPolicy::default()
        };
        policy
            .enabled_ids
            .insert("/tmp/a\tb.desktop:open".to_string());
        policy
            .disabled_ids
            .insert("/tmp/c\\d.desktop:hide".to_string());

        save_service_menu_policy_to(&policy, &path).unwrap();

        assert_eq!(load_service_menu_policy_from(&path), policy);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn load_policy_ignores_corrupt_rows() {
        let path = test_path("corrupt").join("service-menu-policy.tsv");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "mode\tunknown\n\
             enabled\tgood\\\\id\n\
             enabled\tbad\\xid\n\
             disabled\tgood\\tid\n",
        )
        .unwrap();

        let policy = load_service_menu_policy_from(&path);

        assert_eq!(policy.mode, ServiceMenuPolicyMode::AllExceptDisabled);
        assert!(policy.enabled_ids.contains("good\\id"));
        assert!(!policy.enabled_ids.contains("badxid"));
        assert!(policy.disabled_ids.contains("good\tid"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    fn test_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!(
            "fika-service-menu-policy-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}
