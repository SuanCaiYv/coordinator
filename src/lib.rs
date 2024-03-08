pub mod pool;

#[cfg(test)]
mod tests {

    use std::thread;

    use crate::pool;

    #[test]
    fn it_works() {
        let pool = pool::Builder::new().scale_size(1).maximum_size(1).build();
        let submitter = pool.new_submitter();
        let t1 = thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap()
                .block_on(async {
                    for i in 0..100 {
                        _ = submitter
                            .submit(move || {
                                println!("task {:02} in {}", i, thread_id::get());
                            })
                            .await;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                });
        });
        let submitter = pool.new_submitter();
        let t2 = thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap()
                .block_on(async {
                    for i in 100..200 {
                        let s = submitter.clone();
                        tokio::spawn(async move {
                            _ = s
                                .submit(move || {
                                    thread::sleep(std::time::Duration::from_millis(5));
                                    println!("task {:03} in {}", i, thread_id::get());
                                })
                                .await;
                        });
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                });
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }
}
