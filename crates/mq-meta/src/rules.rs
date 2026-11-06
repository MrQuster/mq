//! Mojang OS, architecture, and feature rule models and evaluation logic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::models::RuleContext;

/// Operating system predicate inside a Mojang rule.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsCondition {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

/// Raw Mojang conditional rule (`action: "allow" | "disallow"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawRule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsCondition>,
    #[serde(default)]
    pub features: Option<BTreeMap<String, bool>>,
}

/// Alias for [`RawRule`].
pub type Rule = RawRule;

/// Normalizes OS names to canonical tokens (`"linux"`, `"osx"`, `"windows"`).
pub fn normalize_os_name(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("linux") || trimmed.eq_ignore_ascii_case("gnu/linux") {
        "linux"
    } else if trimmed.eq_ignore_ascii_case("osx")
        || trimmed.eq_ignore_ascii_case("macos")
        || trimmed.eq_ignore_ascii_case("mac")
        || trimmed.eq_ignore_ascii_case("darwin")
    {
        "osx"
    } else if trimmed.eq_ignore_ascii_case("windows")
        || trimmed.eq_ignore_ascii_case("win")
        || trimmed.eq_ignore_ascii_case("win32")
        || trimmed.eq_ignore_ascii_case("win64")
    {
        "windows"
    } else {
        trimmed
    }
}

/// Normalizes CPU architecture identifiers to canonical tokens (`"x86_64"`, `"x86"`, `"arm64"`).
pub fn normalize_arch(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("x86_64")
        || trimmed.eq_ignore_ascii_case("amd64")
        || trimmed.eq_ignore_ascii_case("x64")
    {
        "x86_64"
    } else if trimmed.eq_ignore_ascii_case("x86")
        || trimmed.eq_ignore_ascii_case("i386")
        || trimmed.eq_ignore_ascii_case("i686")
        || trimmed.eq_ignore_ascii_case("ia32")
    {
        "x86"
    } else if trimmed.eq_ignore_ascii_case("arm64") || trimmed.eq_ignore_ascii_case("aarch64") {
        "arm64"
    } else {
        trimmed
    }
}

/// Returns `true` if a single [`RawRule`]'s predicates match the given [`RuleContext`].
pub fn rule_matches(rule: &RawRule, ctx: &RuleContext) -> bool {
    if let Some(ref os_cond) = rule.os {
        if let Some(ref name) = os_cond.name {
            let rule_os = normalize_os_name(name);
            let ctx_os = normalize_os_name(&ctx.os_name);
            if !rule_os.eq_ignore_ascii_case(ctx_os) {
                return false;
            }
        }
        if let Some(ref arch) = os_cond.arch {
            let rule_arch = normalize_arch(arch);
            let ctx_arch = normalize_arch(&ctx.os_arch);
            if !rule_arch.eq_ignore_ascii_case(ctx_arch) {
                return false;
            }
        }
    }

    if let Some(ref req_features) = rule.features {
        for (feature_key, expected_val) in req_features {
            let actual_val = ctx.features.get(feature_key).copied().unwrap_or(false);
            if actual_val != *expected_val {
                return false;
            }
        }
    }

    true
}

/// Evaluates an ordered list of Mojang rules against `ctx`.
///
/// - If `rules` is `None` or empty, returns `true`.
/// - Otherwise starts with `allowed = false` and applies each matching rule in order:
///   `allowed = (rule.action == "allow")`.
pub fn evaluate_rules(rules: Option<&[RawRule]>, ctx: &RuleContext) -> bool {
    let Some(rule_slice) = rules else {
        return true;
    };
    if rule_slice.is_empty() {
        return true;
    }

    let mut allowed = false;
    for rule in rule_slice {
        if rule_matches(rule, ctx) {
            allowed = rule.action.trim().eq_ignore_ascii_case("allow");
        }
    }
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_none_rules_allow_by_default() {
        let ctx = RuleContext::linux_x86_64();
        assert!(evaluate_rules(None, &ctx));
        assert!(evaluate_rules(Some(&[]), &ctx));
    }

    #[test]
    fn allow_all_except_osx_allows_linux() {
        let json = r#"[
            { "action": "allow" },
            { "action": "disallow", "os": { "name": "osx" } }
        ]"#;
        let rules: Vec<RawRule> = serde_json::from_str(json).expect("parse rules");
        let linux_ctx = RuleContext::linux_x86_64();
        assert!(evaluate_rules(Some(&rules), &linux_ctx));

        let mac_ctx = RuleContext {
            os_name: "macos".to_string(),
            os_arch: "aarch64".to_string(),
            features: BTreeMap::new(),
        };
        assert!(!evaluate_rules(Some(&rules), &mac_ctx));
    }

    #[test]
    fn osx_or_windows_only_rule_disallows_linux() {
        let osx_json = r#"[
            { "action": "allow", "os": { "name": "osx" } }
        ]"#;
        let osx_rules: Vec<RawRule> = serde_json::from_str(osx_json).expect("parse osx rules");
        let linux_ctx = RuleContext::linux_x86_64();
        assert!(!evaluate_rules(Some(&osx_rules), &linux_ctx));

        let win_json = r#"[
            { "action": "allow", "os": { "name": "windows" } }
        ]"#;
        let win_rules: Vec<RawRule> = serde_json::from_str(win_json).expect("parse win rules");
        assert!(!evaluate_rules(Some(&win_rules), &linux_ctx));
    }

    #[test]
    fn arch_and_os_normalization_variants_match() {
        let json = r#"[
            { "action": "allow", "os": { "name": "linux", "arch": "amd64" } }
        ]"#;
        let rules: Vec<RawRule> = serde_json::from_str(json).expect("parse rules");
        let linux_x64 = RuleContext::linux_x86_64();
        assert!(evaluate_rules(Some(&rules), &linux_x64));

        let linux_arm64 = RuleContext {
            os_name: "linux".to_string(),
            os_arch: "aarch64".to_string(),
            features: BTreeMap::new(),
        };
        assert!(!evaluate_rules(Some(&rules), &linux_arm64));

        let arm_rule_json = r#"[
            { "action": "allow", "os": { "name": "linux", "arch": "arm64" } }
        ]"#;
        let arm_rules: Vec<RawRule> = serde_json::from_str(arm_rule_json).expect("parse arm rules");
        assert!(evaluate_rules(Some(&arm_rules), &linux_arm64));
        assert!(!evaluate_rules(Some(&arm_rules), &linux_x64));
    }

    #[test]
    fn feature_rules_require_matching_feature_flags() {
        let json = r#"[
            { "action": "allow", "features": { "is_demo_user": true } }
        ]"#;
        let rules: Vec<RawRule> = serde_json::from_str(json).expect("parse feature rules");

        let mut ctx = RuleContext::linux_x86_64();
        assert!(!evaluate_rules(Some(&rules), &ctx));

        ctx.features.insert("is_demo_user".to_string(), true);
        assert!(evaluate_rules(Some(&rules), &ctx));

        ctx.features.insert("is_demo_user".to_string(), false);
        assert!(!evaluate_rules(Some(&rules), &ctx));
    }
}
