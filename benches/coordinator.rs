use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use coordinator::{test_automatic, test_manual, test_unblocking};

fn bench_pools(c: &mut Criterion) {
    let mut group = c.benchmark_group("ThreadPool");
    group.sample_size(200);
    group.bench_function(BenchmarkId::new("Manual", ""),
                         |b| b.iter(|| test_manual()));
    group.bench_function(BenchmarkId::new("Automatic", ""),
                         |b| b.iter(|| test_automatic()));
    group.bench_function(BenchmarkId::new("Unblocking", ""),
                         |b| b.iter(|| test_unblocking()));
    group.finish();
}

criterion_group!(benches, bench_pools);
criterion_main!(benches);