//! A hierarchical timer wheel
//!
//! The number of wheels in the hierarchy are determined by the number of Resolutions passed into
//! the concrete constructors `AllocWheel::new()` and `CopyWheel::new()`. The size of each wheel is
//! automatically determined based on whether certain other resolutions are available. For instance
//! if a wheel is constructed consisting of `Resolution::TenMs` and `Resolution::HundredMs`, then
//! the number of slots in the 10 ms wheel will be 10 (10 slots to get to 100ms). However, if
//! `Resolution::HundredMs` was not used, then `Resolution::TenMs` would have 100 slots (100ms to
//! get to 1 sec).
//!
//! In order for the timer to operate correctly, it must tick at the maximum resolution. For
//! instance if 10ms and 1s resolutions are used, `expire()` must be called every 10ms.
//!
//! The minimum length of a timer is limited by the highest resolution. For instance if 10ms and 1s
//! resolutions were used, the minimum length of a timer would be 10ms.
//!
//! The maximum length of a timer is limited by the lowest resolution. For instance if 10ms, and 1s
//! resolutions were used, the maximum length of a timer would be 59s.
//!
//! There is no migration between wheels. A timer is assigned to a single wheel and is scheduled at
//! it's minimum resolution. E.g. If a timer is scheduled for 1.3s it will be scheduled to
//! fire 2 second ticks later. This is most useful for coarse grain timers, is more efficient
//! computationally and uses less memory than being more precise. The wheels don't have to keep
//! track of offsets for the next inner wheel for wheel to wheel migration, and thus save memory.
//! And since the migration ddoesn't actually occur, we save cpu, and potentially
//! extra allocations.

extern crate time;

mod alloc_wheel;
mod copy_wheel;

pub use alloc_wheel::AllocWheel;
pub use copy_wheel::CopyWheel;

use std::hash::Hash;
use std::fmt::Debug;
use time::Duration;

/// A resolution for a wheel in the hierarchy
///
/// The tick rate of the wheel must match the highest resolution of the wheel.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Resolution {
    Ms,
    TenMs,
    HundredMs,
    Sec,
    Min,
    Hour
}

pub trait Wheel<T: Eq + Hash + Debug + Clone> {
    fn start(&mut self, key: T, time: Duration);
    fn stop(&mut self, key: T);
    fn expire(&mut self) -> Vec<T>;
}

/// An entry in a InnerWheel
#[derive(Debug, Clone)]
struct Slot<T: Debug + Clone> {
    pub entries: Vec<T>
}

impl<T: Debug + Clone> Slot<T> {
    pub fn new() -> Slot<T> {
        Slot {
            entries: Vec::new()
        }
    }
}

/// A wheel at a single resolution
struct InnerWheel<T: Debug + Clone> {
    pub slots: Vec<Slot<T>>
}

impl<T: Debug + Clone> InnerWheel<T> {
    pub fn new(size: usize) -> InnerWheel<T> {
        InnerWheel {
            slots: vec![Slot::new(); size]
        }
    }
}

// Determine the wheel size for each resolution.
//
// Wheel sizes less than one second are adjusted based on the next lowest resolution so that
// resolutions don't overlap.
#[doc(hidden)]
pub fn wheel_sizes(resolutions: &mut Vec<Resolution>) -> Vec<usize> {
    assert!(resolutions.len() > 0);
    resolutions.sort();
    resolutions.dedup();
    let end = resolutions.len() - 1;
    let mut sizes = Vec::with_capacity(resolutions.len());
    for i in 0..resolutions.len() {
        let wheel_size = match resolutions[i] {
            Resolution::Ms => {
                if i == end {
                    1000
                } else {
                    match resolutions[i+1] {
                        Resolution::TenMs => 10,
                        Resolution::HundredMs => 100,
                        _ => 1000
                    }
                }
            },
            Resolution::TenMs => {
                if i == end {
                    100
                } else {
                    match resolutions[i+1] {
                        Resolution::HundredMs => 10,
                        _ => 100
                    }
                }
            },
            Resolution::HundredMs => 10,
            Resolution::Sec => 60,
            Resolution::Min => 60,
            Resolution::Hour => 24
        };
        sizes.push(wheel_size);
    }
    sizes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolutions_sorted_and_deduped() {
        let mut resolutions = vec![Resolution::Sec, Resolution::Min, Resolution::TenMs, Resolution::Min];
        let _ = wheel_sizes(&mut resolutions);
        assert_eq!(vec![Resolution::TenMs, Resolution::Sec, Resolution::Min], resolutions);
    }

    #[test]
    fn wheel_sizes_correct() {
        let mut resolutions = vec![
            vec![Resolution::Ms, Resolution::TenMs, Resolution::Sec],
            vec![Resolution::Ms, Resolution::HundredMs, Resolution::Sec, Resolution::Min],
            vec![Resolution::Ms, Resolution::Sec],
            vec![Resolution::TenMs, Resolution::HundredMs, Resolution::Sec]
        ];

        let expected = vec![
            vec![10, 100, 60],
            vec![100, 10, 60, 60],
            vec![1000, 60],
            vec![10, 10, 60]
        ];

        for (mut r, expected) in resolutions.iter_mut().zip(expected) {
            assert_eq!(expected, wheel_sizes(&mut r));
        }
    }
}
