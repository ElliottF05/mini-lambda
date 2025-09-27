use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

/// A simple RAII-style ticket for tracking the number of jobs in the worker.
pub struct JobTicket {
    counter: Arc<AtomicUsize>,
}

impl JobTicket {
    /// Acquire a new ticket, incrementing the counter.
    pub fn acquire(counter: Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        JobTicket { counter }
    }
}

impl Drop for JobTicket {
    /// Release the ticket, decrementing the counter.
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}




// Unit tests for QueueTicket
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    #[test]
    fn acquire_increments_and_drop_decrements() {
        let counter = Arc::new(AtomicUsize::new(0));

        {
            let _t1 = JobTicket::acquire(counter.clone());
            assert_eq!(counter.load(Ordering::SeqCst), 1);

            {
                let _t2 = JobTicket::acquire(counter.clone());
                assert_eq!(counter.load(Ordering::SeqCst), 2);

                // t2 drops at end of this inner scope
            }
            assert_eq!(counter.load(Ordering::SeqCst), 1);

            // t1 drops at end of outer scope
        }

        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn multiple_tickets_work_and_independent_scopes() {
        let counter = Arc::new(AtomicUsize::new(0));

        let t1 = JobTicket::acquire(counter.clone());
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        let t2 = JobTicket::acquire(counter.clone());
        assert_eq!(counter.load(Ordering::SeqCst), 2);

        drop(t1);
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        drop(t2);
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }
}