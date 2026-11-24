//! Deserialisation helpers shared by the config's value types.

use std::fmt;

use serde::Deserializer;
use serde::de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};

/// What a string-or-list value accepts, as worded in error messages.
#[derive(Clone, Copy)]
pub(crate) struct Expected {
    /// What one item is, such as "a glob string".
    pub(crate) item: &'static str,
    /// The whole value, such as "a glob string or a list of them".
    pub(crate) value: &'static str,
    /// The error for a table, which is reserved for matchers or combinators not supported yet.
    pub(crate) table: &'static str,
}

/// Deserialises a string, or a list of strings, into a list.
///
/// A hand-written visitor rather than an untagged enum, so that a table gets `expected.table` as
/// its error, instead of "data did not match any variant of untagged enum".
pub(crate) fn strings<'de, D: Deserializer<'de>>(
    deserializer: D,
    expected: Expected,
) -> Result<Vec<String>, D::Error> {
    deserializer.deserialize_any(StringsVisitor {
        expected,
        list: true,
    })
}

/// [`strings`] as a [`DeserializeSeed`], for a value read from inside another visitor.
pub(crate) struct Strings(pub(crate) Expected);

impl<'de> DeserializeSeed<'de> for Strings {
    type Value = Vec<String>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        strings(deserializer, self.0)
    }
}

struct StringsVisitor {
    expected: Expected,
    /// Whether a list is accepted, which it isn't for an item inside a list.
    list: bool,
}

impl<'de> Visitor<'de> for StringsVisitor {
    type Value = Vec<String>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(if self.list {
            self.expected.value
        } else {
            self.expected.item
        })
    }

    fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(vec![value.to_owned()])
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        if !self.list {
            return Err(Error::invalid_type(serde::de::Unexpected::Seq, &self));
        }
        let mut items = Vec::new();
        let item = StringsVisitor {
            expected: self.expected,
            list: false,
        };
        while let Some(mut value) = seq.next_element_seed(&item)? {
            items.append(&mut value);
        }
        Ok(items)
    }

    fn visit_map<A: MapAccess<'de>>(self, _map: A) -> Result<Self::Value, A::Error> {
        Err(Error::custom(self.expected.table))
    }
}

impl<'de> DeserializeSeed<'de> for &StringsVisitor {
    type Value = Vec<String>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(StringsVisitor {
            expected: self.expected,
            list: self.list,
        })
    }
}
