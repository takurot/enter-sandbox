use std::fs;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

const CODE_PATH: &str = "/sandbox/__agentbox_internal__/code.py";
const SPIN_DIRECTIVE_PREFIX: &str = "__agentbox_spin_ms=";

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

fn main() {
    let code = read_code();
    maybe_spin(&code);

    // Simulate execution
    println!("Start Execution");
    println!("Executing code: {}", code);
    println!("End Execution");
    let _ = std::io::stdout().flush();
}
