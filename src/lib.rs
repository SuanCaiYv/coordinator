mod pool;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[inline]
pub fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 1,
        1 => 1,
        n => fibonacci(n-1) + fibonacci(n-2),
    }
}

pub fn test_manual() {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async move {
            let pool = pool::manual::ThreadPool::new(24, 40, 102400, false);

            let n: u64 = 100000;
            let mut total = 0;
            for i in 0..n {
                let res = pool.submit(Box::new(move || i)).await.unwrap();
                total += res;
            }
            println!("manual {}", total);
        });
}

pub fn test_automatic() {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async move {
            let pool = pool::automatic::ThreadPool::new(24, 40, 102400);

            let n: u64 = 100000;
            let mut total = 0;
            for i in 0..n {
                total += pool.submit(Box::new(move || i)).await.unwrap();
            }
            println!("automatic {}", total);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        test_automatic();
    }
}
