use std::fs;
use std::io::{Read, Write};

const CODE_PATH: &str = "/sandbox/code.py";

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

fn main() {
    let code = read_code();

    // Simulate execution
    println!("Start Execution");
    println!("Executing code: {}", code);
    println!("End Execution");
    let _ = std::io::stdout().flush();
}
