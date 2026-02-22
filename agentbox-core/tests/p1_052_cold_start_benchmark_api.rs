#[test]
fn benchmark_entrypoint_executes_code() {
    let result = agentbox_core::run_code_once_for_benchmark("print('bench')");
    assert!(result.is_ok(), "benchmark entrypoint failed: {result:?}");
}
