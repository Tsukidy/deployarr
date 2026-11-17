use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;
use core::ops::Deref;

use serde::de::{self, DeserializeSeed, Deserializer};
use serde::{Deserialize, Serialize, Serializer};

/// A list that holds at least one item. It is written as a JSON array, and an
/// empty array fails to parse.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NonEmpty<T>(Vec<T>);

impl<T> NonEmpty<T> {
    /// A list of one item.
    pub fn new(first: T) -> Self {
        Self(vec![first])
    }

    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<T> {
        self.0
    }
}

impl<T> Deref for NonEmpty<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        &self.0
    }
}

impl<'a, T> IntoIterator for &'a NonEmpty<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<T> TryFrom<Vec<T>> for NonEmpty<T> {
    type Error = EmptyListError;

    fn try_from(items: Vec<T>) -> Result<Self, Self::Error> {
        if items.is_empty() {
            Err(EmptyListError)
        } else {
            Ok(Self(items))
        }
    }
}

impl<T> From<NonEmpty<T>> for Vec<T> {
    fn from(list: NonEmpty<T>) -> Self {
        list.0
    }
}

impl<T: Serialize> Serialize for NonEmpty<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for NonEmpty<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        NonEmptySeed(PhantomData::<Vec<T>>).deserialize(deserializer)
    }
}

/// Reads a list with the seed it wraps, then fails if the list is empty.
///
/// The error comes from inside the list's own deserializer, so the path of a
/// parse error names the list, such as `condition.all`.
pub(crate) struct NonEmptySeed<S>(pub(crate) S);

impl<'de, T, S> DeserializeSeed<'de> for NonEmptySeed<S>
where
    S: DeserializeSeed<'de, Value = Vec<T>>,
{
    type Value = NonEmpty<T>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let items = self.0.deserialize(deserializer)?;
        NonEmpty::try_from(items).map_err(|_| de::Error::invalid_length(0, &"at least one item"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyListError;

impl fmt::Display for EmptyListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the list must hold at least one item")
    }
}

impl core::error::Error for EmptyListError {}
