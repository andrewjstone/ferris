use std::iter::Iterator;
use std::rc::{Rc, Weak};
use std::hash::Hash;
use std::collections::HashSet;
use std::mem;
use std::fmt::Debug;
use std::time::Duration;
use super::{InnerWheel, Wheel, Resolution, wheel_sizes};

/// This wheel requires an allocation for each timer as it creates an Rc<T> for its key. This allows
/// the key to be stored in a global hashset that can be used for O(1) cancel. A `Weak<T>` is stored
/// in the wheel slot, so that if the timer is cancelled, the memory is de-allocatd. When the expiry
/// for that slot comes around, an attempt to promote the Weak reference will return `None` and so
/// it will be ignored when draining the wheel slot. If the timer expires before it is cancelled,
/// the weak reference can be used to remove the Rc<T> from the HashMap, as well as trigger the user
/// timeout behavior.
///
/// The minimum duration of a timer is 1 ms.
/// The maximum duration of a timer is 1 day.
pub struct AllocWheel<T: Eq + Hash + Debug + Clone> {
    resolutions: Vec<Resolution>,
    keys: HashSet<Rc<T>>,
    wheels: Vec<InnerWheel<Weak<T>>>,
    slot_indexes: Vec<usize>,
}

impl<T: Eq + Hash + Debug + Clone> AllocWheel<T> {

    /// Create a set of hierarchical inner wheels
    ///
    /// The wheel must be driven by calling `expire` at the maximum resolution.
    /// For example if the maximum resolution is 10ms, then expire must be called every 10ms.
    ///
    /// The maximum value of the wheel is its minimum resolution times the number of slots in that
    /// resolution's wheel. For example if the maximum resolution is 1 second then the max timer
    /// that may be represented is 1 minute, since the second wheel always only contains 60 slots.
    /// If larger timer durations are desired, the user should add another, lower resolution.
    /// The absolute maximum timer duration is 1 day.
    pub fn new(mut resolutions: Vec<Resolution>) -> AllocWheel<T> {
        let sizes = wheel_sizes(&mut resolutions);
        let indexes = vec![0; sizes.len()];
        AllocWheel {
            resolutions: resolutions,
            keys: HashSet::new(),
            wheels: sizes.iter().map(|size| InnerWheel::new(*size)).collect(),
            slot_indexes: indexes
        }
    }

    fn insert_hours(&mut self, key: Weak<T>, time: Duration) -> Result<(), (Weak<T>, Duration)> {
        let slot = time.as_secs()/3600;
        self.insert(key, time, Resolution::Hour, slot as usize + 1)
    }

    fn insert_minutes(&mut self, key: Weak<T>, time: Duration) -> Result<(), (Weak<T>, Duration)> {
        let slot = time.as_secs()/60;
        self.insert(key, time, Resolution::Min, slot as usize + 1)
    }

    fn insert_seconds(&mut self, key: Weak<T>, time: Duration) -> Result<(), (Weak<T>, Duration)> {
        self.insert(key, time, Resolution::Sec, time.as_secs() as usize + 1)
    }

    fn insert_hundred_ms(&mut self, key: Weak<T>, time: Duration) -> Result<(), (Weak<T>, Duration)> {
        let slot = time.subsec_nanos()/(1000*1000*100);
        self.insert(key, time, Resolution::HundredMs, slot as usize + 1)
    }

    fn insert_ten_ms(&mut self, key: Weak<T>, time: Duration) -> Result<(), (Weak<T>, Duration)> {
        let slot = time.subsec_nanos()/(1000*1000*10);
        self.insert(key, time, Resolution::TenMs, slot  as usize + 1)
    }

    fn insert_ms(&mut self, key: Weak<T>, time: Duration) -> Result<(), (Weak<T>, Duration)> {
        let slot = time.subsec_nanos()/(1000*1000);
        self.insert(key, time, Resolution::Ms, slot as usize + 1)
    }

    fn insert(&mut self,
              key: Weak<T>,
              time: Duration,
              resolution: Resolution,
              mut slot: usize) -> Result<(), (Weak<T>, Duration)>
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

impl<T: Eq + Hash + Debug + Clone> Wheel<T> for AllocWheel<T> {
    /// Start a timer with the given duration.
    fn start(&mut self, key: T, time: Duration) {
        let key = Rc::new(key);
        let weak = Rc::downgrade(&key.clone());
        self.keys.insert(key);
        let _ = self.insert_hours(weak, time)
            .or_else(|(weak, time)| self.insert_minutes(weak, time))
            .or_else(|(weak, time)| self.insert_seconds(weak, time))
            .or_else(|(weak, time)| self.insert_hundred_ms(weak, time))
            .or_else(|(weak, time)| self.insert_ten_ms(weak, time))
            .or_else(|(weak, time)| self.insert_ms(weak, time));
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
                           .filter_map(|key| key.upgrade())
                           .filter(|key| keys.remove(key))
                           .map(|key| Rc::try_unwrap(key).unwrap()));

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
    use std::rc::Weak;
    use super::*;
    use std::time::Duration;
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
            Duration::from_millis(5),
            Duration::from_millis(35),
            Duration::from_millis(150),
            Duration::from_secs(5) + Duration::from_millis(10),
            Duration::from_secs(5*60) + Duration::from_secs(10),
            Duration::from_secs(5*3600) + Duration::from_secs(10)
        ];

        let keys = vec!["a", "b", "c", "d", "e", "f"];

        (resolutions, times, keys)
    }

    #[test]
    fn start_and_expire() {
        let (resolutions, times, keys) = values();
        let mut wheel = AllocWheel::new(resolutions);
        for (key, time) in keys.into_iter().zip(times) {
            wheel.start(key, time);
        }
        verify_expire(&mut wheel);
    }

    #[test]
    fn start_and_stop_then_expire() {
        let (resolutions, times, keys) = values();
        let mut wheel = AllocWheel::new(resolutions);
        for (key, time) in keys.clone().into_iter().zip(times) {
            wheel.start(key, time);
        }
        verify_wheel_and_slot_position(&mut wheel);
        for key in keys {
            wheel.stop(key);
        }
        verify_expire_contains_only_weak_refs(&mut wheel);
    }

    fn verify_wheel_and_slot_position(wheel: &mut AllocWheel<&'static str>) {
        let (_, _, keys) = values();
        let expected_slots = [6, 4, 2, 6, 6, 6];
        for i in 0..wheel.wheels.len() {
            for j in 0..wheel.wheels[i].slots.len() {
                let ref entries = wheel.wheels[i].slots[j].entries;
                if j == expected_slots[i] {
                    assert_eq!(1, entries.len());
                    let entry = Weak::upgrade(&entries[0].clone()).unwrap();
                    assert_eq!(keys[i], *entry);
                } else {
                    assert_eq!(0, entries.len());
                }
            }
        }
    }

    fn verify_expire_contains_only_weak_refs(wheel: &mut AllocWheel<&'static str>) {
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

    fn verify_expire(wheel: &mut AllocWheel<&'static str>) {
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
