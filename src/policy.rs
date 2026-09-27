use crate::config::PolicyConfig;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub matched_pattern: Option<String>,
}

pub fn evaluate(policy: &PolicyConfig, full_name: &str) -> PolicyDecision {
    if let Some(pattern) = policy.deny.iter().find(|p| matches(p, full_name)) {
        return PolicyDecision {
            allowed: false,
            matched_pattern: Some(pattern.clone()),
        };
    }
    if let Some(pattern) = policy.allow.iter().find(|p| matches(p, full_name)) {
        return PolicyDecision {
            allowed: true,
            matched_pattern: Some(pattern.clone()),
        };
    }
    PolicyDecision {
        allowed: policy.default == "allow",
        matched_pattern: None,
    }
}

pub fn matches(pattern: &str, full_name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let (Some((pattern_ns, pattern_tool)), Some((namespace, tool))) =
        (pattern.split_once('.'), full_name.split_once('.'))
    else {
        return false;
    };
    glob_component(pattern_ns, namespace) && glob_component(pattern_tool, tool)
}

fn glob_component(pattern: &str, text: &str) -> bool {
    // Linear-space wildcard matching; '*' stays within the component supplied by split_once.
    let (mut p, mut t, mut star, mut retry) = (0usize, 0usize, None, 0usize);
    let pbytes = pattern.as_bytes();
    let tbytes = text.as_bytes();
    while t < tbytes.len() {
        if p < pbytes.len() && pbytes[p] != b'*' && pbytes[p] == tbytes[t] {
            p += 1;
            t += 1;
        } else if p < pbytes.len() && pbytes[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = t;
        } else if let Some(s) = star {
            p = s + 1;
            retry += 1;
            t = retry;
        } else {
            return false;
        }
    }
    while p < pbytes.len() && pbytes[p] == b'*' {
        p += 1;
    }
    p == pbytes.len()
}

#[cfg(test)]
mod tests {
    use super::{evaluate, matches};
    use crate::config::PolicyConfig;

    #[test]
    fn deny_rules_override_allow_rules() {
        let policy = PolicyConfig {
            default: "deny".into(),
            allow: vec!["github.*".into()],
            deny: vec!["*.delete_*".into()],
        };

        assert_eq!(
            evaluate(&policy, "github.list_issues"),
            super::PolicyDecision {
                allowed: true,
                matched_pattern: Some("github.*".into()),
            }
        );
        assert_eq!(
            evaluate(&policy, "github.delete_issue"),
            super::PolicyDecision {
                allowed: false,
                matched_pattern: Some("*.delete_*".into()),
            }
        );
        assert!(!evaluate(&policy, "drive.search_files").allowed);
    }

    #[test]
    fn wildcard_matching_stays_within_namespace_and_tool_components() {
        assert!(matches("*.delete_*", "github.delete_issue"));
        assert!(matches("git*.*issue", "github.delete_issue"));
        assert!(!matches("github.delete_*", "github.admin.delete_issue"));
        assert!(!matches("github.*", "githubish.list_issues"));
        assert!(matches("*", "any.tool.name"));
    }
}
