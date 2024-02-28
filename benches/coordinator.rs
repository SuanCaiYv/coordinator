use std::io::Write;

use blocking::unblock;
use coordinator::pool;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

pub fn test_manual() -> u64 {
    let res = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(async move {
            let mut pool = pool::Builder::new()
                .scale_size(200)
                .queue_size(102400)
                .maximum_size(400)
                .background(false)
                .build_manual();

            let n: u64 = 1500;
            let mut total: u64 = 0;
            for _i in 0..n {
                let res = pool
                    .submit(Box::new(move || simulate_block_call()))
                    .await
                    .unwrap();
                total += res;
            }
            pool.shutdown().await;
            return total;
        });
    return res;
}

pub fn test_automatic() -> u64 {
    return tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async move {
            let pool = pool::Builder::new()
                .scale_size(200)
                .queue_size(102400)
                .maximum_size(400)
                .build_automatic();

            let n: u64 = 1500;
            let mut total = 0;
            for _i in 0..n {
                total += pool
                    .submit(Box::new(move || simulate_block_call()))
                    .await
                    .unwrap();
            }
            return total;
        });
}

pub fn test_unblocking() -> u64 {
    return tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async move {
            let n: u64 = 1500;
            let mut total = 0;
            for _i in 0..n {
                total += unblock(move || simulate_block_call()).await;
            }
            return total;
        });
}

pub fn test_thread_pool() -> u64 {
    return tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async move {
            let pool = threadpool::Builder::new().num_threads(200).build();

            let n: u64 = 1500;
            let mut total = 0;
            for _i in 0..n {
                let (mut tx, rx) = async_oneshot::oneshot();
                pool.execute(move || _ = tx.send(simulate_block_call()));
                total += rx.await.unwrap();
            }
            return total;
        });
}

pub fn test_rayon() -> u64 {
    return tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async move {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(200)
                .build()
                .unwrap();

            let n: u64 = 1500;
            let mut total = 0;
            for _i in 0..n {
                let (mut tx, rx) = async_oneshot::oneshot();
                pool.install(move || {
                    _ = tx.send(simulate_block_call());
                });
                total += rx.await.unwrap();
            }
            return total;
        });
}

pub fn simulate_block_call() -> u64 {
    let t = chrono::Local::now().timestamp_nanos_opt().unwrap() as u64;
    let path = format!("block_test/{}", t);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.write(path.as_bytes()).unwrap();
    return t;
}

fn bench_pools(c: &mut Criterion) {
    let mut group = c.benchmark_group("ThreadPool");
    group.sample_size(10);
    group.bench_function(BenchmarkId::new("Manual", ""), |b| b.iter(|| test_manual()));
    group.bench_function(BenchmarkId::new("Automatic", ""), |b| {
        b.iter(|| test_automatic())
    });
    group.bench_function(BenchmarkId::new("Unblocking", ""), |b| {
        b.iter(|| test_unblocking())
    });
    // group.bench_function(BenchmarkId::new("Threadpool", ""),
    //                      |b| b.iter(|| test_thread_pool()));
    // group.bench_function(BenchmarkId::new("Rayon", ""),
    //                      |b| b.iter(|| test_rayon()));
    group.finish();
}

criterion_group!(benches, bench_pools);
criterion_main!(benches);
