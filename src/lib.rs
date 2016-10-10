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

/// Determine the wheel size for each resolution.
///
/// Wheel sizes less than one second are adjusted based on the next lowest resolution so that
/// resolutions don't overlap.
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

pub trait Wheel<T: Eq + Hash + Debug + Clone> {
    fn start(&mut self, key: T, time: Duration);
    fn stop(&mut self, key: T);
    fn expire(&mut self) -> Vec<T>;
}

pub use alloc_wheel::AllocWheel;

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
