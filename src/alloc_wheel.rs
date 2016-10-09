use std::iter::Iterator;
use std::rc::{Rc, Weak};
use std::hash::Hash;
use std::collections::HashSet;
use std::mem;
use std::fmt::Debug;
use time::Duration;
use super::{Wheel, Resolution, wheel_sizes};

/// An entry in a InnerWheel
struct Slot<T: Eq + Hash + Debug> {
    pub entries: Vec<Weak<T>>
}

/// A wheel at a single resolution
struct InnerWheel<T: Eq + Hash + Debug> {
    pub slots: Vec<Slot<T>>
}

impl<T: Eq + Hash + Debug> InnerWheel<T> {
    pub fn new(size: usize) -> InnerWheel<T> {
        InnerWheel {
            slots: Vec::with_capacity(size)
        }
    }
}

/// This wheel requires an allocation for each timer as it creates an Rc<T> for its key. This allows
/// the key to be stored in a global hashmap that can be used for O(1) cancel. A `Weak<T>` is stored
/// in the wheel slot, so that if the timer is cancelled, the memory is de-allocatd. When the expiry
/// for that slot comes around, an attempt to promote the Weak reference will return `None` and so
/// it will be ignored when draining the wheel slot. If the timer expires before it is cancelled,
/// the weak reference can be used to remove the Rc<T> from the HashMap, as well as trigger the user
/// timeout behavior.
///
/// The minimum duration of a timer is 1 ms.
/// The maximum duration of a timer is 1 day.
pub struct AllocWheel<T: Eq + Hash + Debug> {
    resolutions: Vec<Resolution>,
    keys: HashSet<Rc<T>>,
    wheels: Vec<InnerWheel<T>>,
    slot_indexes: Vec<usize>,
}

impl<T: Eq + Hash + Debug> AllocWheel<T> {

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
        self.insert(key, time, Resolution::Hour, time.num_hours() as usize)
    }

    fn insert_minutes(&mut self, key: Weak<T>, time: Duration) -> Result<(), (Weak<T>, Duration)> {
        self.insert(key, time, Resolution::Min, time.num_minutes() as usize)
    }

    fn insert_seconds(&mut self, key: Weak<T>, time: Duration) -> Result<(), (Weak<T>, Duration)> {
        self.insert(key, time, Resolution::Sec, time.num_seconds() as usize)
    }

    fn insert_hundred_ms(&mut self, key: Weak<T>, time: Duration) -> Result<(), (Weak<T>, Duration)> {
        self.insert(key, time, Resolution::HundredMs, time.num_milliseconds() as usize / 100)
    }

    fn insert_ten_ms(&mut self, key: Weak<T>, time: Duration) -> Result<(), (Weak<T>, Duration)> {
        self.insert(key, time, Resolution::TenMs, time.num_milliseconds()  as usize / 10)
    }

    fn insert_ms(&mut self, key: Weak<T>, time: Duration) -> Result<(), (Weak<T>, Duration)> {
        self.insert(key, time, Resolution::Ms, time.num_milliseconds() as usize)
    }

    fn insert(&mut self,
              key: Weak<T>,
              time: Duration,
              resolution: Resolution,
              mut val: usize) -> Result<(), (Weak<T>, Duration)>
    {
        if val == 0 { return Err((key, time)); }
        if let Some(wheel_index) = self.resolutions.iter().rposition(|ref r| **r == resolution) {
            let max_val = self.wheels[wheel_index].slots.len();
            if val > max_val {
                val = max_val
            }
            let slot_index = (self.slot_indexes[wheel_index] + val as usize) % max_val;
            self.wheels[wheel_index].slots[slot_index].entries.push(key);
            return Ok(());
        }
        Err((key, time))
    }
}

impl<T: Eq + Hash + Debug> Wheel<T> for AllocWheel<T> {
    /// Start a timer with the given duration.
    ///
    /// It will be rounded to the nearest resolution and put in a slot in that resolution's wheel.
    /// Note that any timer with a duration over one-hour will silently be rounded down to 1 hour.
    /// Any timer with a duration less than 10ms will be silently rounded up to 10ms.
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
