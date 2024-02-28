pub mod automatic;
pub mod manual;

pub struct Builder {
    scale_size: usize,
    queue_size: usize,
    stack_size: usize,
    maximum_size: usize,
    background: bool,
}

impl Builder {
    pub fn new() -> Builder {
        Builder {
            scale_size: 200,
            queue_size: 102400,
            stack_size: 1024 * 1024 * 2,
            maximum_size: 1024,
            background: false,
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

    /// - `background` - Whether the thread pool is a background thread.
    /// If you are using async context, recommend `true`.
    pub fn background(mut self, background: bool) -> Builder {
        self.background = background;
        self
    }

    pub fn build_manual<T: Send + Sync>(self) -> manual::ThreadPool<T> {
        manual::ThreadPool::new(self.scale_size, self.queue_size, self.maximum_size, self.stack_size, self.background)
    }

    pub fn build_automatic<T: Send + Sync>(self) -> automatic::ThreadPool<T> {
        automatic::ThreadPool::new(self.scale_size, self.queue_size, self.maximum_size, self.stack_size)
    }
}