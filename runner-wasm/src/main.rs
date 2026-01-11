use std::io::{Read, Write};

fn main() {
    let mut code = String::new();
    if let Err(_) = std::io::stdin().read_to_string(&mut code) {
        return;
    }
    
    // Simulate execution
    println!("Start Execution");
    println!("Executing code: {}", code);
    println!("End Execution");
}
