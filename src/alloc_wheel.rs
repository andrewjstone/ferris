use std::iter::Iterator;
use std::rc::{Rc, Weak};
use std::hash::Hash;
use std::collections::HashSet;
use std::mem;
use std::fmt::Debug;
use time::Duration;
use super::Wheel;

/// This wheel requires an allocation for each timer as it creates an Rc<T> for its key. This allows
/// the key to be stored in a global hashmap that can be used for O(1) cancel. A `Weak<T>` is stored
/// in the wheel slot, so that if the timer is cancelled, the memory is de-allocatd. When the expiry
/// for that slot comes around, an attempt to promote the Weak reference will return `None` and so
/// it will be ignored when draining the wheel slot. If the timer expires before it is cancelled,
/// the weak reference can be used to remove the Rc<T> from the HashMap, as well as trigger the user
/// timeout behavior.
///
/// The minimum duration of a timer is 10ms.
/// The maximum duration of a timer is 1 hour.
pub struct AllocWheel<T: Eq + Hash + Debug> {
    keys: HashSet<Rc<T>>,
    ten_ms_wheel: Vec<Vec<Weak<T>>>,
    s_wheel: Vec<Vec<Weak<T>>>,
    m_wheel: Vec<Vec<Weak<T>>>,
    ten_ms_index: usize,
    s_index: usize,
    m_index: usize
}

impl<T: Eq + Hash + Debug> AllocWheel<T> {
    pub fn new() -> AllocWheel<T> {
        AllocWheel {
            keys: HashSet::new(),
            ten_ms_wheel: vec![Vec::new(); 100],
            s_wheel: vec![Vec::new(); 60],
            m_wheel: vec![Vec::new(); 60],
            ten_ms_index: 0,
            s_index: 0,
            m_index: 0
        }
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
        if time.num_minutes() > 0 {
            let index = (self.m_index + time.num_minutes() as usize) % 60;
            return self.m_wheel.get_mut(index).unwrap().push(weak);
        }
        if time.num_seconds() > 0 {
            let index = (self.s_index + time.num_seconds() as usize) % 60;
            return self.s_wheel.get_mut(index).unwrap().push(weak);
        }
        let mut increment = time.num_milliseconds() as usize / 10;
        if increment < 1 {
            increment = 1;
        }
        let index = (self.ten_ms_index + increment) % 100;
        return self.ten_ms_wheel.get_mut(index).unwrap().push(weak);
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

        self.ten_ms_index = (self.ten_ms_index + 1) % 100;
        let mut expired: Vec<T> = self.ten_ms_wheel.get_mut(self.ten_ms_index).unwrap().drain(..)
            .filter_map(|key| key.upgrade())
            .filter(|key| keys.remove(key)).map(|key| Rc::try_unwrap(key).unwrap()).collect();

        // The ten_ms wheel has rolled over to a second
        if self.ten_ms_index == 0 {
            self.s_index = (self.s_index + 1) % 60;
            expired.extend(self.s_wheel.get_mut(self.s_index).unwrap().drain(..)
                           .filter_map(|key| key.upgrade())
                           .filter(|key| keys.remove(key))
                           .map(|key| Rc::try_unwrap(key).unwrap()));
        }

        // The second wheel has rolled over to a minute
        if self.s_index == 0 {
            self.m_index = (self.m_index + 1) % 60;
            expired.extend(self.m_wheel.get_mut(self.m_index).unwrap().drain(..)
                           .filter_map(|key| key.upgrade())
                           .filter(|key| keys.remove(key))
                           .map(|key| Rc::try_unwrap(key).unwrap()));
        }

        // Make keys part of self again
        mem::swap(&mut keys, &mut self.keys);

        expired
    }
}
