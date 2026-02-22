use std::fs;
use std::io::{Read, Write};
use std::path::Path;
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

fn maybe_write_files(code: &str) {
    for line in code.lines() {
        let line = line.trim();
        if let Some(payload) = line.strip_prefix(WRITE_FILE_DIRECTIVE_PREFIX) {
            if let Some((path, content)) = payload.split_once(':') {
                let full_path = format!("/sandbox/{}", path);
                if let Some(parent) = Path::new(&full_path).parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(full_path, content);
            }
        }
    }
}

fn maybe_read_files(code: &str) {
    for line in code.lines() {
        let line = line.trim();
        if let Some(path) = line.strip_prefix(READ_FILE_DIRECTIVE_PREFIX) {
            let full_path = format!("/sandbox/{}", path);
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
