use std::{
    sync::{atomic::AtomicUsize, Arc},
    thread,
    time::Duration,
};

use crate::pool::Task;
use flume::TrySendError;
use parking_lot::RwLock;
use thread_local::ThreadLocal;
use tracing::warn;

pub struct ThreadPool<T>
where
    T: Send + Sync + 'static,
{
    scheduler: Arc<ThreadLocal<ThreadPoolScheduler<T>>>,
    max_size: usize,
    scale_size: usize,
    queue_size: usize,
    stack_size: usize,
    workers_rx: Arc<RwLock<Vec<flume::Receiver<Task<T>>>>>,
}

impl<T> ThreadPool<T>
where
    T: Send + Sync + 'static,
{
    pub fn new(scale_size: usize, queue_size: usize, max_size: usize, stack_size: usize) -> Self {
        Self {
            scheduler: Arc::new(ThreadLocal::new()),
            max_size,
            scale_size,
            queue_size,
            stack_size,
            workers_rx: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// you can clone a existing `Submitter`, which is equal to `ThreadPool::new_submitter`.
    pub fn new_submitter(&self) -> Submitter<T> {
        Submitter {
            pool: self.scheduler.clone(),
            scale_size: self.scale_size,
            queue_size: self.queue_size,
            maximum_size: self.max_size,
            stack_size: self.stack_size,
            outer_rxs: self.workers_rx.clone(),
        }
    }
}

pub(self) struct ThreadPoolScheduler<T: Send + Sync + 'static> {
    max_size: usize,
    scale_size: usize,
    stack_size: usize,
    current_size: Arc<AtomicUsize>,
    inner_tx: flume::Sender<Task<T>>,
    inner_rx: flume::Receiver<Task<T>>,
    outer_rxs: Arc<RwLock<Vec<flume::Receiver<Task<T>>>>>,
}

impl<T> ThreadPoolScheduler<T>
where
    T: Send + Sync + 'static,
{
    pub(self) fn new(
        scale_size: usize,
        queue_size: usize,
        max_size: usize,
        stack_size: usize,
        outer_rxs: Arc<RwLock<Vec<flume::Receiver<Task<T>>>>>,
    ) -> Self {
        let (inner_tx, inner_rx) = flume::bounded(queue_size);
        let current_size = Arc::new(AtomicUsize::new(0));
        Self {
            max_size,
            scale_size,
            stack_size,
            current_size,
            inner_tx,
            inner_rx,
            outer_rxs,
        }
    }

    pub(self) async fn submit<F>(&self, f: F) -> T
    where
        F: FnOnce() -> T + Send + Sync + 'static,
    {
        let (sender, receiver) = async_oneshot::oneshot();
        let mut task = Task::new(f, sender);

        if self.current_size.load(std::sync::atomic::Ordering::Acquire) < self.scale_size {
            self.new_thread();
            if let Err(e) = self.inner_tx.send_async(task).await {
                warn!("dispatch task error, run directly");
                e.0.run();
            }
        } else if let Err(e) = self.inner_tx.try_send(task) {
            task = match e {
                TrySendError::Full(task) => task,
                TrySendError::Disconnected(task) => task,
            };
            if self.current_size.load(std::sync::atomic::Ordering::Acquire) < self.max_size {
                self.new_thread();
                if let Err(e) = self.inner_tx.send_async(task).await {
                    warn!("dispatch task with more threads error, run directly");
                    e.0.run();
                }
            } else {
                warn!("thread pool is full, run directly");
                task.run();
            }
        }
        receiver.await.unwrap()
    }

    pub(self) fn execute<F>(&self, f: F) -> T
    where
        F: FnOnce() -> T + Send + Sync + 'static,
    {
        return futures_lite::future::block_on(self.submit(f));
    }

    #[inline(always)]
    pub(self) fn new_thread(&self) {
        let inner_rx = self.inner_rx.clone();
        let outer_rxs = self.outer_rxs.clone();
        let current_size = self.current_size.clone();

        let current = current_size.fetch_add(1, std::sync::atomic::Ordering::AcqRel);

        if current < self.max_size {
            _ = thread::Builder::new()
                .name(format!("thread-pool-worker-{:03?}", current))
                .stack_size(self.stack_size)
                .spawn(move || {
                    loop {
                        if let Ok(task) = inner_rx.try_recv() {
                            task.run();
                            continue;
                        }

                        if let Ok(task) = inner_rx.recv_timeout(Duration::from_millis(500)) {
                            task.run();
                            continue;
                        }

                        let mut peer_tasks = Vec::new();
                        {
                            let outer_rxs = outer_rxs.read();
                            let list = &*outer_rxs;
                            for rx in list.iter() {
                                match rx.try_recv() {
                                    Ok(task) => {
                                        peer_tasks.push(task);
                                    }
                                    Err(_) => {}
                                }
                            }
                        }
                        let mut flag = false;
                        for task in peer_tasks {
                            flag = true;
                            task.run();
                        }
                        if flag {
                            continue;
                        }

                        // this number is from golang scheduler.
                        match inner_rx.recv_timeout(Duration::from_secs(61)) {
                            Ok(task) => {
                                task.run();
                            }
                            Err(_) => {
                                break;
                            }
                        }
                    }
                    current_size.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
                });
        } else {
            self.current_size
                .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        }
    }
}

#[derive(Clone)]
pub struct Submitter<T>
where
    T: Send + Sync + 'static,
{
    pool: Arc<ThreadLocal<ThreadPoolScheduler<T>>>,
    scale_size: usize,
    queue_size: usize,
    maximum_size: usize,
    stack_size: usize,
    outer_rxs: Arc<RwLock<Vec<flume::Receiver<Task<T>>>>>,
}

impl<T> Submitter<T>
where
    T: Send + Sync + 'static,
{
    pub async fn submit<F>(&self, f: F) -> T
    where
        F: FnOnce() -> T + Send + Sync + 'static,
    {
        self.pool
            .get_or(|| {
                let pool = ThreadPoolScheduler::new(
                    self.scale_size,
                    self.queue_size,
                    self.maximum_size,
                    self.stack_size,
                    self.outer_rxs.clone(),
                );
                self.outer_rxs.write().push(pool.inner_rx.clone());
                pool
            })
            .submit(f)
            .await
    }

    pub fn execute<F>(&self, f: F) -> T
    where
        F: FnOnce() -> T + Send + Sync + 'static,
    {
        self.pool
            .get_or(|| {
                let pool = ThreadPoolScheduler::new(
                    self.scale_size,
                    self.queue_size,
                    self.maximum_size,
                    self.stack_size,
                    self.outer_rxs.clone(),
                );
                self.outer_rxs.write().push(pool.inner_rx.clone());
                pool
            })
            .execute(f)
    }
}
