use crate::config::Task;
use chrono::prelude::*;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::{Arc, Condvar, Mutex};

pub struct Queue {
    queue: Mutex<BinaryHeap<QueuedTask>>,
    condvar: Condvar,
}

#[derive(Clone)]
pub struct QueuedTask {
    pub time: NaiveDateTime,
    pub task: Arc<Task>,
}

impl PartialEq for QueuedTask {
    fn eq(&self, other: &Self) -> bool {
        NaiveDateTime::eq(&self.time, &other.time)
    }
}

impl Eq for QueuedTask {}

impl PartialOrd for QueuedTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(QueuedTask::cmp(self, other))
    }
}

impl Ord for QueuedTask {
    fn cmp(&self, other: &Self) -> Ordering {
        NaiveDateTime::cmp(&self.time, &other.time).reverse()
    }
}

impl Queue {
    pub fn new() -> Arc<Queue> {
        Arc::new(Queue {
            queue: Mutex::default(),
            condvar: Condvar::default(),
        })
    }

    pub fn notify_push(self: &Arc<Self>, time: NaiveDateTime, task: Arc<Task>) {
        eprintln!("[{}] next run {:}", &task.name, time.format("%Y-%m-%d %H:%M:%S"));
        self.queue.lock().unwrap().push(QueuedTask { time, task });
        self.condvar.notify_all();
    }

    pub fn wait_peek_time(self: &Arc<Self>) -> NaiveDateTime {
        let mut queue_lock = self.queue.lock().unwrap();
        while queue_lock.is_empty() {
            queue_lock = self.condvar.wait(queue_lock).unwrap();
        }
        queue_lock.peek().unwrap().time
    }

    pub fn try_pop(&self) -> Option<Arc<Task>> {
        self.queue
            .lock().unwrap()
            .pop()
            .map(|qt| qt.task.clone())
    }
}
