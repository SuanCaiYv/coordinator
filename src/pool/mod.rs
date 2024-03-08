
pub mod automatic;

pub(self) struct Task<T: 'static> {
    pub(self) f: Box<dyn FnOnce() -> T + Send + Sync + 'static>,
    pub(self) sender: async_oneshot::Sender<T>,
}

impl<T> Task<T> {
    pub(self) fn new<F>(f: F, sender: async_oneshot::Sender<T>) -> Self
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

pub struct Builder {
    scale_size: usize,
    queue_size: usize,
    stack_size: usize,
    maximum_size: usize,
}

impl Builder {
    pub fn new() -> Builder {
        Builder {
            scale_size: 40,
            queue_size: 102400,
            stack_size: 1024 * 1024 * 2,
            maximum_size: 80,
        }
    }

    /// - `scale_size` - The scale size of the thread pool when all threads are busy.
    pub fn scale_size(mut self, scale_size: usize) -> Builder {
        self.scale_size = scale_size;
        self
    }

    /// - `queue_size` - The size of the queue for the thread pool.
    /// If target is `manual::ThreadPool<T>`, then the size apply for every thread.
    /// If target is `automatic::ThreadPool<T>`, then the size apply for the whole pool.
    pub fn queue_size(mut self, queue_size: usize) -> Builder {
        self.queue_size = queue_size;
        self
    }

    /// - `stack_size` - The stack size of the thread pool.
    /// If the tasks are small, recommend 2MB.
    #[allow(dead_code)]
    pub fn stack_size(mut self, stack_size: usize) -> Builder {
        self.stack_size = stack_size;
        self
    }

    /// - `maximum_size` - The maximum size of the thread pool.
    pub fn maximum_size(mut self, maximum_size: usize) -> Builder {
        self.maximum_size = maximum_size;
        self
    }

    pub fn build<T: Send + Sync>(self) -> automatic::ThreadPool<T> {
        automatic::ThreadPool::new(self.scale_size, self.queue_size, self.maximum_size, self.stack_size)
    }
}