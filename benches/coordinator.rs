use std::sync::Arc;
use std::thread;

use blocking::unblock;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use coordinator::pool;

pub fn test_automatic() -> u64 {
    return tokio::runtime::Builder::new_multi_thread()
        .build()
        .unwrap()
        .block_on(async move {
            let m = 1024;
            let n = 10;
            let pool = pool::Builder::new()
                .scale_size(60)
                .queue_size(10240000)
                .maximum_size(80)
                .build();

            let (tx, rx) = flume::bounded(m);
            for _ in 0..m {
                let tx = tx.clone();
                let submitter = pool.new_submitter();
                tokio::spawn(async move {
                    let mut total = 0;
                    for _i in 0..n {
                        total += submitter
                            .submit(move || simulate_block_call())
                            .await;
                    }
                    tx.send(total).unwrap();
                });
            }

            let mut total = 0;
            for _ in 0..m {
                total += rx.recv_async().await.unwrap();
            }
            return total;
        });
}

pub fn test_unblocking() -> u64 {
    return tokio::runtime::Builder::new_multi_thread()
        .build()
        .unwrap()
        .block_on(async move {
            let m = 1024;
            let n = 10;

            let (tx, rx) = flume::bounded(m);
            for _ in 0..m {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let mut total = 0;
                    for _i in 0..n {
                        total += unblock(move || simulate_block_call()).await;
                    }
                    tx.send(total).unwrap();
                });
            }

            let mut total = 0;
            for _ in 0..m {
                total += rx.recv_async().await.unwrap();
            }
            return total;
        });
}

pub fn test_thread_pool() -> u64 {
    return tokio::runtime::Builder::new_multi_thread()
        .build()
        .unwrap()
        .block_on(async move {
            let m = 1024;
            let n = 10;
            let pool = threadpool::Builder::new().num_threads(800).build();

            let (tx, rx) = flume::bounded(m);
            for _ in 0..m {
                let tx = tx.clone();
                let pool = pool.clone();
                tokio::spawn(async move {
                    let mut total = 0;
                    for _i in 0..n {
                        let (mut tx, rx) = async_oneshot::oneshot();
                        pool.execute(move || _ = tx.send(simulate_block_call()));
                        total += rx.await.unwrap();
                    }
                    tx.send(total).unwrap();
                });
            }

            let mut total = 0;
            for _ in 0..m {
                total += rx.recv_async().await.unwrap();
            }
            return total;
        });
}

pub fn test_rayon() -> u64 {
    return tokio::runtime::Builder::new_multi_thread()
        .build()
        .unwrap()
        .block_on(async move {
            let m = 1024;
            let n = 10;
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(800)
                .build()
                .unwrap();
            let pool = Arc::new(pool);

            let (tx, rx) = flume::bounded(m);
            for _ in 0..m {
                let tx = tx.clone();
                let pool = pool.clone();
                tokio::spawn(async move {
                    let mut total = 0;
                    for _i in 0..n {
                        let (mut tx, rx) = async_oneshot::oneshot();
                        pool.install(move || {
                            _ = tx.send(simulate_block_call());
                        });
                        total += rx.await.unwrap();
                    }
                    tx.send(total).unwrap();
                });
            }

            let mut total = 0;
            for _ in 0..m {
                total += rx.recv_async().await.unwrap();
            }
            return total;
        });
}

pub fn simulate_block_call() -> u64 {
    return 1;
}

fn bench_pools(c: &mut Criterion) {
    let mut group = c.benchmark_group("ThreadPool");
    group.sample_size(50);
    group.bench_function(BenchmarkId::new("Automatic", ""), |b| {
        b.iter(|| test_automatic())
    });
    group.bench_function(BenchmarkId::new("Unblocking", ""), |b| {
        b.iter(|| test_unblocking())
    });
    group.bench_function(BenchmarkId::new("Threadpool", ""), |b| {
        b.iter(|| test_automatic())
    });
    group.bench_function(BenchmarkId::new("Rayon", ""), |b| {
        b.iter(|| test_unblocking())
    });
    group.finish();
}

criterion_group!(benches, bench_pools);
criterion_main!(benches);
