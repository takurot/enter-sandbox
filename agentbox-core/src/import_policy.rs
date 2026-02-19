//! Static import-policy enforcement for `SandboxConfig.allowed_modules`.
//!
//! # Security note
//!
//! This module performs **static, lexical analysis** of the submitted Python
//! source code.  It is intentionally a *convenience guard*, not a security
//! boundary.  Dynamic import forms such as
//!
//! ```python
//! __import__("os")
//! importlib.import_module("os")
//! exec("import os")
//! ```
//!
//! are **not** detected and will not be blocked by this check.
//!
//! The actual security boundary is the WebAssembly (WASM) runtime: all user
//! code runs inside a Wasmtime sandbox with capability-based isolation.
//! `allowed_modules` is an ergonomic API that surfaces policy violations early
//! (before the WASM engine starts) and produces a clear error message, but it
//! must not be relied upon as the sole defence against malicious code.

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

    for statement in split_statements(code) {
        let candidate = statement.trim_start();
        if let Some(rest) = strip_keyword(candidate, "import") {
            parse_import_clause(rest, &mut imports);
            continue;
        }
        if let Some(rest) = strip_keyword(candidate, "from") {
            parse_from_import_clause(rest, &mut imports);
        }
    }

    imports.into_iter().collect()
}

fn split_statements(code: &str) -> Vec<&str> {
    let mut statements = Vec::new();
    let bytes = code.as_bytes();
    let mut start = 0usize;
    let mut index = 0usize;
    let mut state = StringState::Outside;
    let mut in_comment = false;
    let mut escaped = false;

    while index < bytes.len() {
        let byte = bytes[index];

        if in_comment {
            if byte == b'\n' {
                statements.push(&code[start..index]);
                start = index + 1;
                in_comment = false;
            }
            index += 1;
            continue;
        }

        match state {
            StringState::Outside => match byte {
                b'#' => {
                    in_comment = true;
                }
                b'\'' => {
                    if is_triple_quote(bytes, index, b'\'') {
                        state = StringState::TripleSingle;
                        index += 3;
                        continue;
                    }
                    state = StringState::Single;
                }
                b'"' => {
                    if is_triple_quote(bytes, index, b'"') {
                        state = StringState::TripleDouble;
                        index += 3;
                        continue;
                    }
                    state = StringState::Double;
                }
                b';' | b'\n' => {
                    statements.push(&code[start..index]);
                    start = index + 1;
                }
                _ => {}
            },
            StringState::Single => {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'\'' {
                    state = StringState::Outside;
                }
            }
            StringState::Double => {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    state = StringState::Outside;
                }
            }
            StringState::TripleSingle => {
                if is_triple_quote(bytes, index, b'\'') {
                    state = StringState::Outside;
                    index += 3;
                    continue;
                }
            }
            StringState::TripleDouble => {
                if is_triple_quote(bytes, index, b'"') {
                    state = StringState::Outside;
                    index += 3;
                    continue;
                }
            }
        }
        index += 1;
    }

    statements.push(&code[start..index]);
    statements
}

fn is_triple_quote(bytes: &[u8], index: usize, quote: u8) -> bool {
    bytes.get(index) == Some(&quote)
        && bytes.get(index + 1) == Some(&quote)
        && bytes.get(index + 2) == Some(&quote)
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum StringState {
    Outside,
    Single,
    Double,
    TripleSingle,
    TripleDouble,
}

fn parse_import_clause(rest: &str, imports: &mut BTreeSet<String>) {
    for raw_module in rest.split(',') {
        if let Some(module) = extract_module_name(raw_module) {
            imports.insert(module);
        }
    }
}

fn parse_from_import_clause(rest: &str, imports: &mut BTreeSet<String>) {
    // Strip leading dots for relative imports (e.g. `from . import foo`,
    // `from ..pkg import bar`).  Relative imports reference names within the
    // same package and cannot introduce external modules, so we skip them
    // rather than blocking them.
    let rest_trimmed = rest.trim_start();
    if rest_trimmed.starts_with('.') {
        return;
    }

    // Split on whitespace to find the module name and the `import` keyword.
    // We must handle the parenthesised form:
    //   from collections import (defaultdict, OrderedDict)
    // In that case `parts.next()` after the keyword may start with `(`.
    let mut parts = rest_trimmed.split_whitespace();
    let Some(module_part) = parts.next() else {
        return;
    };
    let Some(keyword) = parts.next() else {
        return;
    };
    // The keyword token is normally "import", but when there is no space
    // before the opening parenthesis it becomes "import(..." (e.g.
    // `from collections import(defaultdict)`).  Accept both forms.
    if !keyword.starts_with("import") {
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
    // Do NOT strip leading dots here; relative-import detection is handled in
    // `parse_from_import_clause` before this function is called.
    let trimmed = raw.trim();
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
        assert!(message.contains("collections"), "message: {message}");
        assert!(message.contains("os"), "message: {message}");
        assert!(message.contains("allowed=[json]"), "message: {message}");
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

    #[test]
    fn ignores_semicolon_import_text_inside_string_literals() {
        let code = "print(\"safe; import os\")\nprint('safe; from collections import deque')";
        let allowed = Vec::<String>::new();
        assert!(enforce_allowed_modules(code, Some(&allowed)).is_ok());
    }

    #[test]
    fn ignores_comment_only_import_text_after_hash() {
        let code = "print('ok')  # import os";
        let allowed = Vec::<String>::new();
        assert!(enforce_allowed_modules(code, Some(&allowed)).is_ok());
    }

    #[test]
    fn ignores_import_text_inside_multiline_triple_quoted_strings() {
        let code = "x = '''safe\n; import os\nstill safe'''\nprint(x)";
        let allowed = Vec::<String>::new();
        assert!(enforce_allowed_modules(code, Some(&allowed)).is_ok());
    }

    // --- Parenthesised from-import form ---

    #[test]
    fn detects_from_import_with_parenthesised_names() {
        let code = "from collections import (\n    defaultdict,\n    OrderedDict,\n)";
        let allowed = Vec::<String>::new();

        let error = enforce_allowed_modules(code, Some(&allowed))
            .expect_err("collections should be detected in parenthesised form");
        assert!(
            error.to_string().contains("collections"),
            "message: {}",
            error
        );
    }

    #[test]
    fn allows_from_import_with_parenthesised_names_when_permitted() {
        let code = "from collections import (\n    defaultdict,\n    OrderedDict,\n)";
        let allowed = vec!["collections".to_string()];
        assert!(enforce_allowed_modules(code, Some(&allowed)).is_ok());
    }

    #[test]
    fn detects_from_import_no_space_before_paren() {
        // `from collections import(defaultdict)` — no space before `(`
        let code = "from collections import(defaultdict)";
        let allowed = Vec::<String>::new();

        let error = enforce_allowed_modules(code, Some(&allowed))
            .expect_err("collections should be detected even without space before paren");
        assert!(
            error.to_string().contains("collections"),
            "message: {}",
            error
        );
    }

    // --- Relative imports ---

    #[test]
    fn ignores_relative_imports_single_dot() {
        // `from . import utils` — relative, no external module introduced
        let code = "from . import utils";
        let allowed = Vec::<String>::new();
        // Relative imports are skipped (not blocked) because they cannot
        // introduce external modules.
        assert!(enforce_allowed_modules(code, Some(&allowed)).is_ok());
    }

    #[test]
    fn ignores_relative_imports_double_dot() {
        let code = "from ..models import Foo";
        let allowed = Vec::<String>::new();
        assert!(enforce_allowed_modules(code, Some(&allowed)).is_ok());
    }
}
