use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path};
use std::time::{Duration, Instant};

const CODE_PATH: &str = "/sandbox/__agentbox_internal__/code.py";
const SPIN_DIRECTIVE_PREFIX: &str = "__agentbox_spin_ms=";
const WRITE_FILE_DIRECTIVE_PREFIX: &str = "__agentbox_write_file=";
const READ_FILE_DIRECTIVE_PREFIX: &str = "__agentbox_read_file=";

fn read_code() -> String {
    if let Ok(code) = fs::read_to_string(CODE_PATH) {
        return code;
    }

    let mut code = String::new();
    if std::io::stdin().read_to_string(&mut code).is_ok() {
        return code;
    }

    String::new()
}

fn parse_spin_directive(code: &str) -> Option<u64> {
    let first_line = code.lines().next()?.trim();
    let value = first_line.strip_prefix(SPIN_DIRECTIVE_PREFIX)?;
    value.parse::<u64>().ok()
}

fn maybe_spin(code: &str) {
    let Some(spin_ms) = parse_spin_directive(code) else {
        return;
    };

    let deadline = Instant::now() + Duration::from_millis(spin_ms);
    while Instant::now() < deadline {
        std::hint::spin_loop();
    }
}

fn normalize_directive_path(path: &str) -> Option<String> {
    let mut normalized_parts = Vec::new();
    for component in Path::new(path.trim().trim_start_matches('/')).components() {
        match component {
            Component::Normal(part) => normalized_parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    if normalized_parts.is_empty() {
        return None;
    }

    Some(normalized_parts.join("/"))
}

fn maybe_write_files(code: &str) {
    for line in code.lines() {
        let line = line.trim_start();
        if let Some(payload) = line.strip_prefix(WRITE_FILE_DIRECTIVE_PREFIX) {
            let Some((raw_path, content)) = payload.split_once(':') else {
                eprintln!("Invalid write directive: {line}");
                continue;
            };

            let Some(path) = normalize_directive_path(raw_path) else {
                eprintln!("Invalid directive path: {}", raw_path.trim());
                continue;
            };

            let full_path = format!("/sandbox/{path}");
            if let Some(parent) = Path::new(&full_path).parent() {
                if let Err(error) = fs::create_dir_all(parent) {
                    eprintln!("Failed to create parent directory for {path}: {error}");
                    continue;
                }
            }

            if let Err(error) = fs::write(&full_path, content) {
                eprintln!("Failed to write {path}: {error}");
            }
        }
    }
}

fn maybe_read_files(code: &str) {
    for line in code.lines() {
        let line = line.trim_start();
        if let Some(raw_path) = line.strip_prefix(READ_FILE_DIRECTIVE_PREFIX) {
            let Some(path) = normalize_directive_path(raw_path) else {
                eprintln!("Invalid directive path: {}", raw_path.trim());
                continue;
            };

            let full_path = format!("/sandbox/{path}");
            if let Ok(content) = fs::read_to_string(full_path) {
                println!("File {}: {}", path, content);
            } else {
                println!("File {} not found", path);
            }
        }
    }
}

fn main() {
    let code = read_code();
    maybe_spin(&code);
    maybe_write_files(&code);
    maybe_read_files(&code);

    // Simulate execution
    println!("Start Execution");
    println!("Executing code: {}", code);
    println!("End Execution");
    let _ = std::io::stdout().flush();
}
