//! State channel system for oxidizedgraph
//!
//! Channels define how state fields are updated when multiple
//! nodes modify the same field.

use serde::{Deserialize, Serialize};

/// Defines how a state field should be updated
#[derive(Clone, Debug)]
pub enum Channel<T> {
    /// Last write wins - new value replaces old
    LastValue(T),

    /// Append to a collection
    Append(Vec<T>),

    /// Custom reducer function
    Reduce {
        /// Current value
        value: T,
        /// Reducer function: (old, new) -> result
        reducer: fn(T, T) -> T,
    },
}

impl<T: Clone> Channel<T> {
    /// Create a new LastValue channel
    pub fn last_value(value: T) -> Self {
        Self::LastValue(value)
    }

    /// Create a new Append channel
    pub fn append(values: Vec<T>) -> Self {
        Self::Append(values)
    }

    /// Create a new Reduce channel
    pub fn reduce(value: T, reducer: fn(T, T) -> T) -> Self {
        Self::Reduce { value, reducer }
    }

    /// Get the current value
    pub fn value(&self) -> &T {
        match self {
            Self::LastValue(v) => v,
            Self::Append(v) => v.first().expect("Append channel must have at least one value"),
            Self::Reduce { value, .. } => value,
        }
    }

    /// Update the channel with a new value
    pub fn update(&mut self, new_value: T) {
        match self {
            Self::LastValue(v) => *v = new_value,
            Self::Append(v) => v.push(new_value),
            Self::Reduce { value, reducer } => {
                let old = value.clone();
                *value = reducer(old, new_value);
            }
        }
    }
}

/// Trait for types that can be reduced
pub trait Reducer<T> {
    /// Reduce two values into one
    fn reduce(a: T, b: T) -> T;
}

/// A reducer that keeps the last value
pub struct LastValueReducer;

impl<T> Reducer<T> for LastValueReducer {
    fn reduce(_a: T, b: T) -> T {
        b
    }
}

/// A reducer that appends values (for Vec types)
pub struct AppendReducer;

impl<T> Reducer<Vec<T>> for AppendReducer {
    fn reduce(mut a: Vec<T>, mut b: Vec<T>) -> Vec<T> {
        a.append(&mut b);
        a
    }
}

/// A reducer that sums numeric values
pub struct SumReducer;

impl Reducer<i64> for SumReducer {
    fn reduce(a: i64, b: i64) -> i64 {
        a + b
    }
}

impl Reducer<f64> for SumReducer {
    fn reduce(a: f64, b: f64) -> f64 {
        a + b
    }
}

/// A reducer that keeps the maximum value
pub struct MaxReducer;

impl Reducer<i64> for MaxReducer {
    fn reduce(a: i64, b: i64) -> i64 {
        a.max(b)
    }
}

impl Reducer<f64> for MaxReducer {
    fn reduce(a: f64, b: f64) -> f64 {
        a.max(b)
    }
}

/// A reducer that keeps the minimum value
pub struct MinReducer;

impl Reducer<i64> for MinReducer {
    fn reduce(a: i64, b: i64) -> i64 {
        a.min(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_last_value_channel() {
        let mut channel = Channel::last_value(1);
        assert_eq!(*channel.value(), 1);

        channel.update(2);
        assert_eq!(*channel.value(), 2);
    }

    #[test]
    fn test_append_channel() {
        let mut channel = Channel::append(vec![1]);
        channel.update(2);
        channel.update(3);

        if let Channel::Append(values) = channel {
            assert_eq!(values, vec![1, 2, 3]);
        } else {
            panic!("Expected Append channel");
        }
    }

    #[test]
    fn test_reduce_channel() {
        let mut channel = Channel::reduce(0i64, |a, b| a + b);
        channel.update(5);
        channel.update(3);

        if let Channel::Reduce { value, .. } = channel {
            assert_eq!(value, 8);
        } else {
            panic!("Expected Reduce channel");
        }
    }

    #[test]
    fn test_reducer_traits() {
        assert_eq!(SumReducer::reduce(5i64, 3i64), 8);
        assert_eq!(MaxReducer::reduce(5i64, 3i64), 5);
        assert_eq!(MinReducer::reduce(5i64, 3i64), 3);
    }
}
