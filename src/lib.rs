//! A hierarchical timer wheel
//!
//! There are 3 wheels in the hierarchy, of resolution 10ms, 1s, and 1m. The max timeout length is 1
//! hour. Any timer scheduled over 1 hour will expire in 1 hour.
//!
//! There is no migration between wheels. A timer is assigned to a single wheel and is scheduled at
//! the max resolution of the wheel. E.g. If a timer is scheduled for 1.3s it will be scheduled to
//! fire 2 second ticks later. This is most useful for coarse grain timers and is more efficient
//! computationally and uses less memory than being more precise. The wheels don't have to keep
//! track of offsets for the next inner wheel so the timer can be rescheduled when the outer wheel
//! slot expires. And it doesn't have to actually do the reschedule, saving cpu, and potentially
//! extra allocations.

extern crate time;

mod alloc_wheel;

use std::hash::Hash;
use std::fmt::Debug;
use time::Duration;

pub trait Wheel<T: Eq + Hash + Debug> {
    fn start(&mut self, key: T, time: Duration);
    fn stop(&mut self, key: T);
    fn expire(&mut self) -> Vec<T>;
}

pub use alloc_wheel::AllocWheel;
