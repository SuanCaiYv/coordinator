use std::{
    thread::{self, JoinHandle},
    time::Duration,
};

use async_oneshot::{Sender as OneshotSender, Receiver as OneshotReceiver};
use crossbeam_channel::TrySendError;
use flume::Sender;
use sysinfo::{System, SystemExt};
use tracing::{debug, warn};

pub(self) struct Task<T: 'static> {
    pub(self) f: Box<dyn FnOnce() -> T + Send + Sync + 'static>,
    pub(self) sender: OneshotSender<T>,
}

impl<T> Task<T> {
    pub(self) fn new<F>(f: F, sender: OneshotSender<T>) -> Self
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
        _ = self.sender.send(res);
    }
}

pub(self) struct Worker {
    id: usize,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub(self) fn new<T: 'static + Send + Sync>(
        id: usize,
        task_receiver: crossbeam_channel::Receiver<Option<Task<T>>>,
    ) -> Self {
        let handle = thread::Builder::new()
            .stack_size(1024 * 1024 * 2)
            .name(format!("thread-pool-worker-{}", id))
            .spawn(move || loop {
                match task_receiver.recv_timeout(Duration::from_secs(61)) {
                    Ok(task) => {
                        if let Some(task) = task {
                            task.run();
                        } else {
                            break;
                        }
                    }
                    Err(_) => {
                        break;
                    }
                }
            })
            .unwrap();
        Self {
            id,
            thread: Some(handle),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.thread.take().unwrap().join().unwrap();
    }
}

/// a thread pool for block syscall
pub struct ThreadPool<T: Send + Sync + 'static> {
    _workers_handle: Option<JoinHandle<()>>,
    inner_tx: Sender<Option<Task<T>>>,
    shutdown_notify: Option<OneshotReceiver<()>>,
}

impl<T: 'static + Sync + Send> ThreadPool<T> {
    /// cache_size: number of task on the fly.
    /// scale_size: max number of threads when all workers are busy.
    /// max_size: max number of threads.
    ///
    /// default size of thread created is equal to number of available cpu cores,
    /// when all workers are busy, new thread will be created until reach the scale size,
    /// when all workers with their sender be fully, create new thread until max size.
    pub fn new(
        scale_size: usize,
        max_size: usize,
        queue_size: usize,
        enable_background_thread: bool,
    ) -> Self {
        let mut sys = System::new();
        sys.refresh_cpu();
        let default_size = sys.cpus().len();

        let (inner_tx, inner_rx) = flume::bounded::<Option<Task<T>>>(default_size * 64);
        let mut workers = Vec::with_capacity(default_size);
        let mut task_senders = Vec::with_capacity(default_size);

        if enable_background_thread {
            // the backend thread which manage the workers
            let workers_handle = thread::spawn(move || {
                for i in 0..default_size {
                    let (task_tx, task_rx) = crossbeam_channel::bounded(queue_size);
                    let worker = Worker::new(i, task_rx);

                    workers.push(worker);
                    task_senders.push(task_tx);
                }
                let mut previous: Option<Task<T>> = None;
                loop {
                    let task = if previous.is_some() {
                        previous.take().unwrap()
                    } else {
                        match inner_rx.recv() {
                            Ok(task) => {
                                if task.is_none() {
                                    for sender in task_senders {
                                        _ = sender.send(None);
                                    }
                                    break;
                                }
                                task.unwrap()
                            }
                            Err(_) => break,
                        }
                    };

                    let mut index = usize::MAX;
                    let mut remain_size = 0;

                    for i in 0..task_senders.len() {
                        let cap = task_senders[i].capacity().unwrap();
                        if cap > remain_size {
                            remain_size = cap;
                            index = i;
                        }
                    }

                    // first, there exists idle worker, select it.
                    if index != usize::MAX {
                        let worker_id = workers[index].id;
                        debug!("send task to idle worker: {}", worker_id);
                        if let Err(e) = task_senders[index].try_send(Some(task)) {
                            match e {
                                TrySendError::Disconnected(mut task) => {
                                    task_senders.remove(index);
                                    workers.remove(index);
                                    previous = Some(task.take().unwrap());
                                }
                                TrySendError::Full(mut task) => {
                                    previous = Some(task.take().unwrap());
                                }
                            }
                            continue;
                        }
                    } else {
                        // second, create new thread for task when allowed.
                        if task_senders.len() < scale_size {
                            let new_worker_id = workers[workers.len() - 1].id + 1;
                            debug!("create new worker: {}", new_worker_id);

                            let (task_tx, task_rx) = crossbeam_channel::bounded(queue_size);
                            let worker = Worker::new(new_worker_id, task_rx);

                            workers.push(worker);
                            _ = task_tx.send(Some(task));
                            task_senders.push(task_tx);
                        } else {
                            // third, send task to the worker which has the most capacity.
                            match task_senders[index].try_send(Some(task)) {
                                Ok(_) => {
                                    debug!("buffer: {} available", workers[index].id);
                                }
                                Err(e) => {
                                    match e {
                                        TrySendError::Full(mut task) => {
                                            // last, create more threads before reach max size.
                                            if workers.len() < max_size {
                                                let new_worker_id =
                                                    workers[workers.len() - 1].id + 1;
                                                debug!("create more worker: {}", new_worker_id);
                                                let (task_tx, task_rx) =
                                                    crossbeam_channel::bounded(queue_size);
                                                let worker = Worker::new(new_worker_id, task_rx);

                                                workers.push(worker);
                                                _ = task_tx.send(task);
                                                task_senders.push(task_tx);
                                            } else {
                                                // unfortunately, the system is busy.
                                                // consider to increase the max size of thread pool or size of cache queue.
                                                warn!(
                                                    "system is busy, queues and threads all full!"
                                                );
                                                // let idx = fastrand::usize(0..workers.len());
                                                // if let Err(e) = task_senders[idx].send(task) {
                                                //     println!("send task error: {:?}", e);
                                                // }
                                                task.take().unwrap().run();
                                            }
                                        }
                                        TrySendError::Disconnected(mut task) => {
                                            task_senders.remove(index);
                                            workers.remove(index);
                                            previous = Some(task.take().unwrap());
                                            continue;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            });
            Self {
                _workers_handle: Some(workers_handle),
                inner_tx,
                shutdown_notify: None,
            }
        } else {
            let (mut shutdown_tx, shutdown_rx) = async_oneshot::oneshot();
            tokio::spawn(async move {
                for i in 0..default_size {
                    let (task_tx, task_rx) = crossbeam_channel::bounded(queue_size);
                    let worker = Worker::new(i, task_rx);

                    workers.push(worker);
                    task_senders.push(task_tx);
                }
                let mut previous: Option<Task<T>> = None;
                loop {
                    let task = if previous.is_some() {
                        previous.take().unwrap()
                    } else {
                        match inner_rx.recv_async().await {
                            Ok(task) => {
                                if task.is_none() {
                                    for sender in task_senders {
                                        _ = sender.send(None);
                                    }
                                    break;
                                }
                                task.unwrap()
                            }
                            Err(_) => {
                                break;
                            }
                        }
                    };

                    let mut index = usize::MAX;
                    let mut remain_size = 0;

                    for i in 0..task_senders.len() {
                        let cap = task_senders[i].capacity().unwrap();
                        if cap > remain_size {
                            remain_size = cap;
                            index = i;
                        }
                    }

                    // first, there exists idle worker, select it.
                    if index != usize::MAX {
                        let worker_id = workers[index].id;
                        debug!("send task to idle worker: {}", worker_id);
                        if let Err(e) = task_senders[index].try_send(Some(task)) {
                            match e {
                                TrySendError::Disconnected(mut task) => {
                                    task_senders.remove(index);
                                    workers.remove(index);
                                    previous = Some(task.take().unwrap());
                                }
                                TrySendError::Full(mut task) => {
                                    previous = Some(task.take().unwrap());
                                }
                            }
                            continue;
                        }
                    } else {
                        // second, create new thread for task when allowed.
                        if task_senders.len() < scale_size {
                            let new_worker_id = workers[workers.len() - 1].id + 1;
                            debug!("create new worker: {}", new_worker_id);

                            let (task_tx, task_rx) = crossbeam_channel::bounded(queue_size);
                            let worker = Worker::new(new_worker_id, task_rx);

                            workers.push(worker);
                            _ = task_tx.try_send(Some(task));
                            task_senders.push(task_tx);
                        } else {
                            // third, send task to the worker which has the most capacity.
                            match task_senders[index].try_send(Some(task)) {
                                Ok(_) => {
                                    debug!("buffer: {} available", workers[index].id);
                                }
                                Err(e) => {
                                    match e {
                                        TrySendError::Full(mut task) => {
                                            // last, create more threads before reach max size.
                                            if workers.len() < max_size {
                                                let new_worker_id =
                                                    workers[workers.len() - 1].id + 1;
                                                debug!("create more worker: {}", new_worker_id);
                                                let (task_tx, task_rx) =
                                                    crossbeam_channel::bounded(queue_size);
                                                let worker = Worker::new(new_worker_id, task_rx);

                                                workers.push(worker);
                                                _ = task_tx.try_send(task);
                                                task_senders.push(task_tx);
                                            } else {
                                                // unfortunately, the system is busy.
                                                // consider to increase the max size of thread pool or size of cache queue.
                                                warn!(
                                                    "system is busy, queues and threads all full!"
                                                );
                                                // let idx = fastrand::usize(0..workers.len());
                                                // if let Err(e) = task_senders[idx].send(task) {
                                                //     println!("send task error: {:?}", e);
                                                // }
                                                task.take().unwrap().run();
                                            }
                                        }
                                        TrySendError::Disconnected(mut task) => {
                                            task_senders.remove(index);
                                            workers.remove(index);
                                            previous = Some(task.take().unwrap());
                                            continue;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                shutdown_tx.send(()).unwrap();
            });
            Self {
                _workers_handle: None,
                inner_tx,
                shutdown_notify: Some(shutdown_rx),
            }
        }
    }

    #[allow(unused)]
    pub fn execute<F>(&self, f: F) -> anyhow::Result<T>
        where
            F: FnOnce() -> T + Send + Sync + 'static,
    {
        return futures_lite::future::block_on(self.submit(f));
    }

    /// if the queue is full and create more threads is not allow, the call will block on async context.
    pub async fn submit<F>(&self, f: F) -> anyhow::Result<T>
        where
            F: FnOnce() -> T + Send + Sync + 'static,
    {
        let (sender, receiver) = async_oneshot::oneshot();
        let task = Task::new(f, sender);
        self.inner_tx.send_async(Some(task)).await?;
        let res = receiver.await;
        if res.is_err() {
            return Err(anyhow::anyhow!("task receiver error"));
        }
        Ok(res.unwrap())
    }

    /// used only for benchmark, cause single thread runtime will not wait for shutdown of all threads.
    pub(crate) async fn exit(&mut self) {
        self.inner_tx.send_async(None).await.unwrap();
        if let Some(notify) = self.shutdown_notify.take() {
            notify.await.unwrap();
        }
    }
}

impl<T: Send + Sync + 'static> Drop for ThreadPool<T> {
    fn drop(&mut self) {
        futures_lite::future::block_on(self.exit());
    }
}
