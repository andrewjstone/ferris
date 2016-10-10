use std::iter::Iterator;
use std::hash::Hash;
use std::collections::HashSet;
use std::mem;
use std::fmt::Debug;
use time::Duration;
use super::{InnerWheel, Wheel, Resolution, wheel_sizes};

/// This wheel maintains a copy of the timer key in both the appropriate inner timer wheel slot and
/// the global hashset. This does not require an allocation for each timer but may use more memory
/// than an CopyWheel depending upon the size of the keys. When the expiry for a slot occurs, the
/// global hashmap is checked for the expiring keys. If they are still there it means they are valid
/// to expire, otherwise they have already been cancelled.
///
/// The minimum duration of a timer is 1 ms.
/// The maximum duration of a timer is 1 day.
pub struct CopyWheel<T: Eq + Hash + Debug + Clone> {
    resolutions: Vec<Resolution>,
    keys: HashSet<T>,
    wheels: Vec<InnerWheel<T>>,
    slot_indexes: Vec<usize>,
}

impl<T: Eq + Hash + Debug + Clone> CopyWheel<T> {

    /// Create a set of hierarchical inner wheels
    ///
    /// The wheel must be driven by calling `expire` at the maximum resolution.
    /// For example if the maximum resolution is 10ms, then expire must be called every 10ms.
    ///
    /// The maximum value of the wheel is its minimum resolution times the number of slots in that
    /// resolution's wheel. For example if the maximum resolution is 1 second then the max timer
    /// that may be represented is 1 minute, since the second wheel always only contains 60 slots.
    /// If larger timer durations are desired, the user should add another, lower resolution, inner
    /// wheel. The absolute  maximum timer duration is 1 day.
    pub fn new(mut resolutions: Vec<Resolution>) -> CopyWheel<T> {
        let sizes = wheel_sizes(&mut resolutions);
        let indexes = vec![0; sizes.len()];
        CopyWheel {
            resolutions: resolutions,
            keys: HashSet::new(),
            wheels: sizes.iter().map(|size| InnerWheel::new(*size)).collect(),
            slot_indexes: indexes
        }
    }

    fn insert_hours(&mut self, key: T, time: Duration) -> Result<(), (T, Duration)> {
        self.insert(key, time, Resolution::Hour, time.num_hours() as usize + 1)
    }

    fn insert_minutes(&mut self, key: T, time: Duration) -> Result<(), (T, Duration)> {
        self.insert(key, time, Resolution::Min, time.num_minutes() as usize + 1)
    }

    fn insert_seconds(&mut self, key: T, time: Duration) -> Result<(), (T, Duration)> {
        self.insert(key, time, Resolution::Sec, time.num_seconds() as usize + 1)
    }

    fn insert_hundred_ms(&mut self, key: T, time: Duration) -> Result<(), (T, Duration)> {
        self.insert(key, time, Resolution::HundredMs, time.num_milliseconds() as usize / 100 + 1)
    }

    fn insert_ten_ms(&mut self, key: T, time: Duration) -> Result<(), (T, Duration)> {
        self.insert(key, time, Resolution::TenMs, time.num_milliseconds()  as usize / 10 + 1)
    }

    fn insert_ms(&mut self, key: T, time: Duration) -> Result<(), (T, Duration)> {
        self.insert(key, time, Resolution::Ms, time.num_milliseconds() as usize + 1)
    }

    fn insert(&mut self,
              key: T,
              time: Duration,
              resolution: Resolution,
              mut slot: usize) -> Result<(), (T, Duration)>
    {
        // The slot will always be at least 2 ahead of the current, since we add one in each of the
        // insert_xxx methods
        if slot == 1 { return Err((key, time)); }
        if let Some(wheel_index) = self.resolutions.iter().rposition(|ref r| **r == resolution) {
            let max_slot = self.wheels[wheel_index].slots.len();
            if slot > max_slot {
                slot = max_slot
            }
            let slot_index = (self.slot_indexes[wheel_index] + slot) % max_slot;
            self.wheels[wheel_index].slots[slot_index].entries.push(key);
            return Ok(());
        }
        Err((key, time))
    }
}

impl<T: Eq + Hash + Debug + Clone> Wheel<T> for CopyWheel<T> {
    /// Start a timer with the given duration.
    ///
    /// It will be rounded to the nearest resolution and put in a slot in that resolution's wheel.
    /// Note that any timer with a duration over one-hour will silently be rounded down to 1 hour.
    /// Any timer with a duration less than 10ms will be silently rounded up to 10ms.
    fn start(&mut self, key: T, time: Duration) {
        self.keys.insert(key.clone());
        let _ = self.insert_hours(key, time)
            .or_else(|(key, time)| self.insert_minutes(key, time))
            .or_else(|(key, time)| self.insert_seconds(key, time))
            .or_else(|(key, time)| self.insert_hundred_ms(key, time))
            .or_else(|(key, time)| self.insert_ten_ms(key, time))
            .or_else(|(key, time)| self.insert_ms(key, time));
    }

    /// Cancel a timer.
    fn stop(&mut self, key: T) {
        self.keys.remove(&key);
    }

    /// Return any expired timer keys
    fn expire(&mut self) -> Vec<T> {
        // Take keys out of self temporarily so we don't have to borrow self
        let mut keys = HashSet::new();
        mem::swap(&mut keys, &mut self.keys);

        let mut expired = Vec::new();
        for (ref mut wheel, ref mut slot_index) in self.wheels.iter_mut().zip(&mut self.slot_indexes) {
            **slot_index = (**slot_index + 1) % wheel.slots.len();
            expired.extend(wheel.slots[**slot_index].entries.drain(..)
                           .filter(|key| keys.remove(key)));

            // We haven't wrapped around to the next wheel
            if **slot_index != 0 {
                break;
            }
        }

        // Make keys part of self again
        mem::swap(&mut keys, &mut self.keys);
        expired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;
    use super::super::{Resolution, Wheel};

    fn values() -> (Vec<Resolution>, Vec<Duration>, Vec<&'static str>) {
        let resolutions = vec![
            Resolution::Ms,
            Resolution::TenMs,
            Resolution::HundredMs,
            Resolution::Sec,
            Resolution::Min,
            Resolution::Hour
        ];

        let times = vec![
            Duration::milliseconds(5),
            Duration::milliseconds(35),
            Duration::milliseconds(150),
            Duration::seconds(5) + Duration::milliseconds(10),
            Duration::minutes(5) + Duration::seconds(10),
            Duration::hours(5) + Duration::seconds(10)
        ];

        let keys = vec!["a", "b", "c", "d", "e", "f"];

        (resolutions, times, keys)
    }

    #[test]
    fn start_and_expire() {
        let (resolutions, times, keys) = values();
        let mut wheel = CopyWheel::new(resolutions);
        for (key, time) in keys.into_iter().zip(times) {
            wheel.start(key, time);
        }
        verify_expire(&mut wheel);
    }

    #[test]
    fn start_and_stop_then_expire() {
        let (resolutions, times, keys) = values();
        let mut wheel = CopyWheel::new(resolutions);
        for (key, time) in keys.clone().into_iter().zip(times) {
            wheel.start(key, time);
        }
        verify_wheel_and_slot_position(&mut wheel);
        for key in keys {
            wheel.stop(key);
        }
        verify_expire_contains_only_weak_refs(&mut wheel);
    }

    fn verify_wheel_and_slot_position(wheel: &mut CopyWheel<&'static str>) {
        let (_, _, keys) = values();
        let expected_slots = [6, 4, 2, 6, 6, 6];
        for i in 0..wheel.wheels.len() {
            for j in 0..wheel.wheels[i].slots.len() {
                let ref entries = wheel.wheels[i].slots[j].entries;
                if j == expected_slots[i] {
                    assert_eq!(1, entries.len());
                    assert_eq!(keys[i], entries[0]);
                } else {
                    assert_eq!(0, entries.len());
                }
            }
        }
    }

    fn verify_expire_contains_only_weak_refs(wheel: &mut CopyWheel<&'static str>) {
        // We only go until the 5 minute timer. We expire wheel 0, index 1 first (hence the -1)
        // The 6 is because we always start an extra slot late because the current one is in
        // progress and we don't want to fire early. So the timer will fire between 5 and 6 minutes
        // in a normal program depending upon current slot positions in the wheels
        let total_ticks = 6*60000 - 1;

        for _ in 0..total_ticks {
            let expired = wheel.expire();
            assert_eq!(0, expired.len());
        }
    }

    fn verify_expire(wheel: &mut CopyWheel<&'static str>) {
        let (_, _, keys) = values();
        let expected_ticks = [
            5, // We always expire starting at slot 1
            4 * 10 - 1, // 4 x 10 ms ticks
            2 * 100 - 1, // 2 x 10 ms ticks x 10 10ms ticks
            6 * 1000 - 1, // 6 x 10 ms ticks * 10 10ms ticks x 10 100ms ticks = 6 * 1 second,
            6 * 60000 - 1, // 6 * 60 seconds (60000 ms) = 6 * 1 minute

            // Skip the last one since it makes the test run for too long
            // 6 * 60 * 60000 - 1 // 6 * 60 minutes
        ];

        let mut match_count = 0;
        for i in 0..expected_ticks[4] {
            let expired = wheel.expire();
            if expected_ticks.contains(&i) {
                assert_eq!(1, expired.len());
                assert_eq!(keys[match_count], expired[0]);
                match_count = match_count + 1;
            } else  {
                assert_eq!(0, expired.len());
            }
        }
    }
}


