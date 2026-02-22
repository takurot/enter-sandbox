use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_tier1_cold_start(c: &mut Criterion) {
    let mut group = c.benchmark_group("tier1_cold_start");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(8));
    group.warm_up_time(Duration::from_secs(2));

    group.bench_function("sandbox_run_print_hello", |b| {
        b.iter(|| {
            agentbox_core::run_code_once_for_benchmark(black_box("print('hello')"))
                .expect("benchmark iteration should succeed");
        });
    });

    group.finish();
}

criterion_group!(benches, bench_tier1_cold_start);
criterion_main!(benches);
