use blocking::unblock;

mod pool;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[inline]
pub fn fibonacci_slow(n: u64) -> u64 {
    match n {
        0 => 1,
        1 => 1,
        n => fibonacci_slow(n - 1) + fibonacci_slow(n - 2),
    }
}

#[inline]
pub fn fibonacci_fast(n: u64) -> u64 {
    let mut a = 0;
    let mut b = 1;

    match n {
        0 => b,
        _ => {
            for _ in 0..n {
                let c = a + b;
                a = b;
                b = c;
            }
            b
        }
    }
}

pub fn test_manual() -> u64 {
    let res = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(async move {
            let mut pool = pool::manual::ThreadPool::new(200, 400, 102400, false);

            let n: u64 = 100000;
            let mut total: u64 = 0;
            for i in 0..n {
                let res = pool.submit(Box::new(move || i)).await.unwrap();
                total += res;
            }
            // pool.exit().await;
            return total;
        });
    return res;
}

pub fn test_automatic() -> u64 {
    return tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async move {
            let pool = pool::automatic::ThreadPool::new(200, 400, 102400);

            let n: u64 = 100000;
            let mut total = 0;
            for i in 0..n {
                total += pool.submit(Box::new(move || i)).await.unwrap();
            }
            return total;
        });
}

pub fn test_unblocking() -> u64 {
    return tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async move {
            let n: u64 = 100000;
            let mut total = 0;
            for i in 0..n {
                total += unblock(move || i).await;
            }
            return total;
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        println!("{}", test_manual());
    }
}
