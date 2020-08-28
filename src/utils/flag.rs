use std::sync::atomic::{AtomicBool, Ordering};

pub struct Flag {
    state: AtomicBool,
}

impl Flag {
    pub const fn new() -> Flag {
        Flag {
            state: AtomicBool::new(false),
        }
    }

    pub fn triggered(&self) -> bool {
        self.state.load(Ordering::Relaxed)
    }

    pub fn trigger(&self) {
        self.state.store(true, Ordering::Relaxed);
    }
}
