use std::{fmt, sync::Arc, thread};

trait TaskSpawnerBackend: Send + Sync {
    fn spawn(&self, task: Box<dyn FnOnce() + Send>);
}

#[derive(Default)]
struct ThreadTaskSpawnerBackend;

impl TaskSpawnerBackend for ThreadTaskSpawnerBackend {
    fn spawn(&self, task: Box<dyn FnOnce() + Send>) {
        thread::spawn(task);
    }
}

#[derive(Clone)]
pub struct TaskSpawner {
    backend: Arc<dyn TaskSpawnerBackend>,
}

impl TaskSpawner {
    pub fn spawn<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.backend.spawn(Box::new(task));
    }
}

impl Default for TaskSpawner {
    fn default() -> Self {
        Self {
            backend: Arc::new(ThreadTaskSpawnerBackend),
        }
    }
}

impl fmt::Debug for TaskSpawner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskSpawner").finish()
    }
}
