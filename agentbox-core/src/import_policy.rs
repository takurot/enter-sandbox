use anyhow::{bail, Result};
use std::collections::BTreeSet;

pub fn enforce_allowed_modules(code: &str, allowed_modules: Option<&[String]>) -> Result<()> {
    let Some(allowed_modules) = allowed_modules else {
        return Ok(());
    };

    let normalized_allowed = normalize_allowed_modules(allowed_modules);
    let imported_modules = collect_imported_modules(code);

    let blocked_modules: Vec<String> = imported_modules
        .into_iter()
        .filter(|module| !is_module_allowed(module, &normalized_allowed))
        .collect();

    if blocked_modules.is_empty() {
        return Ok(());
    }

    let blocked = blocked_modules.join(", ");
    let allowed = if normalized_allowed.is_empty() {
        "(none)".to_string()
    } else {
        normalized_allowed.join(", ")
    };

    bail!(
        "Import blocked by SandboxConfig.allowed_modules. blocked=[{blocked}] allowed=[{allowed}]"
    )
}

fn normalize_allowed_modules(allowed_modules: &[String]) -> Vec<String> {
    let mut normalized = BTreeSet::new();
    for module in allowed_modules {
        let trimmed = module.trim();
        if trimmed.is_empty() {
            continue;
        }
        normalized.insert(trimmed.to_string());
    }
    normalized.into_iter().collect()
}

fn is_module_allowed(module: &str, allowed_modules: &[String]) -> bool {
    allowed_modules.iter().any(|allowed| {
        module == allowed
            || module
                .strip_prefix(allowed)
                .is_some_and(|suffix| suffix.starts_with('.'))
    })
}

fn collect_imported_modules(code: &str) -> Vec<String> {
    let mut imports = BTreeSet::new();

    for line in code.lines() {
        for statement in line.split(';') {
            let candidate = statement.trim_start();
            if let Some(rest) = strip_keyword(candidate, "import") {
                parse_import_clause(rest, &mut imports);
                continue;
            }
            if let Some(rest) = strip_keyword(candidate, "from") {
                parse_from_import_clause(rest, &mut imports);
            }
        }
    }

    imports.into_iter().collect()
}

fn parse_import_clause(rest: &str, imports: &mut BTreeSet<String>) {
    for raw_module in rest.split(',') {
        if let Some(module) = extract_module_name(raw_module) {
            imports.insert(module);
        }
    }
}

fn parse_from_import_clause(rest: &str, imports: &mut BTreeSet<String>) {
    let mut parts = rest.split_whitespace();
    let Some(module_part) = parts.next() else {
        return;
    };
    let Some(keyword) = parts.next() else {
        return;
    };
    if keyword != "import" {
        return;
    }

    let Some(module) = extract_module_name(module_part) else {
        return;
    };
    imports.insert(module);
}

fn strip_keyword<'a>(statement: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = statement.strip_prefix(keyword)?;
    let first = rest.chars().next()?;
    if !first.is_whitespace() {
        return None;
    }
    Some(rest.trim_start())
}

fn extract_module_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_start_matches('.');
    if trimmed.is_empty() {
        return None;
    }

    let module = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.')
        .collect::<String>();
    if module.is_empty() {
        return None;
    }

    Some(module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_all_modules_when_policy_is_unset() {
        let code = "import os\nfrom collections import deque\n";
        assert!(enforce_allowed_modules(code, None).is_ok());
    }

    #[test]
    fn allows_imports_when_all_detected_modules_are_permitted() {
        let code = "import json\nfrom collections import defaultdict\nimport xml.etree.ElementTree";
        let allowed = vec![
            "json".to_string(),
            "collections".to_string(),
            "xml".to_string(),
        ];

        assert!(enforce_allowed_modules(code, Some(&allowed)).is_ok());
    }

    #[test]
    fn blocks_disallowed_modules_and_sorts_message_stably() {
        let code = "import os\nfrom collections import deque\nimport json";
        let allowed = vec!["json".to_string()];

        let error = enforce_allowed_modules(code, Some(&allowed))
            .expect_err("disallowed imports should fail");
        let message = error.to_string();
        assert!(message.contains("blocked=[collections, os]"));
        assert!(message.contains("allowed=[json]"));
    }

    #[test]
    fn empty_allow_list_blocks_any_detected_imports() {
        let code = "import json";
        let allowed = Vec::<String>::new();

        let error = enforce_allowed_modules(code, Some(&allowed))
            .expect_err("imports should be blocked when allow-list is empty");
        assert!(error.to_string().contains("allowed=[(none)]"));
    }

    #[test]
    fn detects_imports_with_tabs_and_semicolons() {
        let code = "value = 1; import\tjson\nfrom\tcollections\timport\tdeque";
        let allowed = vec!["json".to_string()];

        let error = enforce_allowed_modules(code, Some(&allowed))
            .expect_err("collections import should be detected and blocked");
        assert!(error.to_string().contains("blocked=[collections]"));
    }

    #[test]
    fn ignores_text_that_only_mentions_import_in_string_literals() {
        let code = "print(\"import os\")\nprint('from collections import deque')";
        let allowed = Vec::<String>::new();
        assert!(enforce_allowed_modules(code, Some(&allowed)).is_ok());
    }
}
