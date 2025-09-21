use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

/// A simple RAII-style ticket for tracking the number of active jobs in the worker's queue.
pub struct QueueTicket {
    counter: Arc<AtomicUsize>,
}

impl QueueTicket {
    /// Acquire a new ticket, incrementing the counter.
    pub fn acquire(counter: Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        QueueTicket { counter }
    }
}

impl Drop for QueueTicket {
    /// Release the ticket, decrementing the counter.
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}