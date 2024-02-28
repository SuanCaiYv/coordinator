use std::{
    sync::{atomic::AtomicUsize, Arc},
    thread,
    time::Duration,
};

use async_oneshot::Sender as OneshotSender;
use flume::TrySendError;
use tracing::warn;

pub(self) struct Task<T: 'static> {
    pub(self) f: Box<dyn FnOnce() -> T + Send + Sync + 'static>,
    pub(self) sender: OneshotSender<anyhow::Result<T>>,
}

impl<T> Task<T> {
    pub(self) fn new<F>(f: F, sender: OneshotSender<anyhow::Result<T>>) -> Self
        where
            F: FnOnce() -> T + Send + Sync + 'static,
    {
        Self {
            f: Box::new(f),
            sender,
        }
    }

    pub(self) fn run(mut self) {
        let res = (self.f)();
        _ = self.sender.send(Ok(res));
    }
}

#[derive(Clone)]
pub struct ThreadPool<T: Send + Sync + 'static> {
    max_size: usize,
    scale_size: usize,
    stack_size: usize,
    current_size: Arc<AtomicUsize>,
    inner_tx: flume::Sender<Task<T>>,
    inner_rx: flume::Receiver<Task<T>>,
}

impl<T> ThreadPool<T>
    where
        T: Send + Sync + 'static,
{
    pub fn new(scale_size: usize, queue_size: usize, max_size: usize, stack_size: usize) -> Self {
        let (inner_tx, inner_rx) = flume::bounded(queue_size);
        let current_size = Arc::new(AtomicUsize::new(0));
        Self {
            max_size,
            scale_size,
            stack_size,
            current_size,
            inner_tx,
            inner_rx,
        }
    }

    pub async fn submit<F>(&self, f: F) -> anyhow::Result<T>
        where
            F: FnOnce() -> T + Send + Sync + 'static,
    {
        let (sender, receiver) = async_oneshot::oneshot();
        let mut task = Task::new(f, sender);

        if self.current_size.load(std::sync::atomic::Ordering::Relaxed) < self.scale_size {
            self.new_thread();
            self.inner_tx.send_async(task).await?;
        } else if let Err(e) = self.inner_tx.try_send(task) {
            task = match e {
                TrySendError::Full(task) => task,
                TrySendError::Disconnected(task) => task,
            };
            if self.current_size.load(std::sync::atomic::Ordering::Relaxed) < self.max_size {
                self.new_thread();
                self.inner_tx.send_async(task).await?;
            } else {
                warn!("thread pool is full, run directly");
                task.run();
            }
        }
        return match receiver.await {
            Ok(res) => res,
            Err(_) => Err(anyhow::anyhow!("task receiver error")),
        };
    }

    #[allow(dead_code)]
    pub(self) async fn submit_immediately<F>(&self, f: F) -> anyhow::Result<T>
        where
            F: FnOnce() -> T + Send + Sync + 'static,
    {
        let (sender, receiver) = async_oneshot::oneshot();
        let task = Task::new(f, sender);

        if self.current_size.load(std::sync::atomic::Ordering::Relaxed) < self.scale_size {
            self.new_thread();
            if let Err(e) = self.inner_tx.try_send(task) {
                let task = match e {
                    TrySendError::Full(task) => task,
                    TrySendError::Disconnected(task) => task,
                };
                self.inner_tx.send_async(task).await?;
            }
        } else if let Err(e) = self.inner_tx.try_send(task) {
            let task = match e {
                TrySendError::Full(task) => task,
                TrySendError::Disconnected(task) => task,
            };
            if self.current_size.load(std::sync::atomic::Ordering::Relaxed) < self.max_size {
                self.new_thread();
                if let Err(e) = self.inner_tx.try_send(task) {
                    let task = match e {
                        TrySendError::Full(task) => task,
                        TrySendError::Disconnected(task) => task,
                    };
                    self.inner_tx.send_async(task).await?;
                }
            } else {
                warn!("thread pool is full, run directly");
                task.run();
            }
        }
        return match receiver.await {
            Ok(res) => res,
            Err(_) => Err(anyhow::anyhow!("task receiver error")),
        };
    }

    #[allow(dead_code)]
    pub fn execute<F>(&self, f: F) -> anyhow::Result<T>
        where
            F: FnOnce() -> T + Send + Sync + 'static,
    {
        return futures_lite::future::block_on(self.submit(f));
    }

    #[inline(always)]
    pub(self) fn new_thread(&self) {
        let inner_rx = self.inner_rx.clone();
        let current_size = self.current_size.clone();

        let current = current_size.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        if current < self.max_size {
            _ = thread::Builder::new()
                .name(format!("thread-pool-worker-{:03?}", current))
                .stack_size(self.stack_size)
                .spawn(move || {
                    loop {
                        // this number is from golang scheduler.
                        // todo, make as alternative.
                        match inner_rx.recv_timeout(Duration::from_secs(61)) {
                            Ok(task) => {
                                task.run();
                            }
                            Err(_) => {
                                break;
                            }
                        }
                    }
                    current_size.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                });
        } else {
            self.current_size
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}
