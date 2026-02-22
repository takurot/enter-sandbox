use agentbox_core;
use std::env;

#[cfg(target_os = "macos")]
fn get_max_rss_kb() -> Option<i64> {
    let mut usage = unsafe { std::mem::zeroed::<libc::rusage>() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } == 0 {
        Some(usage.ru_maxrss / 1024)
    } else {
        None
    }
}

#[cfg(not(target_os = "macos"))]
#[cfg(unix)]
fn get_max_rss_kb() -> Option<i64> {
    let mut usage = unsafe { std::mem::zeroed::<libc::rusage>() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } == 0 {
        Some(usage.ru_maxrss)
    } else {
        None
    }
}

#[cfg(not(unix))]
fn get_max_rss_kb() -> Option<i64> {
    None
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let scenario = args.get(1).map(|s| s.as_str()).unwrap_or("all");

    println!("--- Tier 1 Memory Usage Benchmark ---");
    println!("NOTE: ru_maxrss is the process lifetime peak. Scenario comparisons should be run in separate processes for best accuracy.");

    if scenario == "all" || scenario == "cold" {
        run_cold_scenario();
    }

    if scenario == "all" || scenario == "warm" {
        run_warm_scenario();
    }
}

fn run_cold_scenario() {
    println!("\n[Scenario A: Cold Start (New Runtime per run)]");
    for i in 0..5 {
        agentbox_core::run_code_once_for_benchmark("print('hello')")
            .expect("sandbox run should succeed");
        
        let current_rss = get_max_rss_kb().unwrap_or(0);
        println!("Cold Run {:2}: Peak RSS: {} KB (process peak)", i + 1, current_rss);
    }
    println!("Final Peak for Cold Scenario: {} KB", get_max_rss_kb().unwrap_or(0));
}

fn run_warm_scenario() {
    println!("\n[Scenario B: Warm Start (Reuse Runtime)]");
    let runtime = agentbox_core::WasmRuntime::new().unwrap();
    let vfs = agentbox_core::VirtualFS::new();
    let config = agentbox_core::default_runtime_config();

    for i in 0..10 {
        agentbox_core::execute_sandbox_run(&runtime, &vfs, &config, "print('hello')")
            .expect("sandbox run should succeed");
        
        let current_rss = get_max_rss_kb().unwrap_or(0);
        println!("Warm Run {:2}: Peak RSS: {} KB (process peak)", i + 1, current_rss);
    }
    println!("Final Peak for Warm Scenario: {} KB", get_max_rss_kb().unwrap_or(0));
}
