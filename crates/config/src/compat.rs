//! A `config.toml` that another Browsitory version wrote (#1137). The desktop app and the VS Code
//! extension update on their own, so an older version regularly reads and writes a newer one's
//! file (and a newer one an older one's after a downgrade).
//!
//! - Reading: a value of a closed setting this version doesn't know (`word_wrap = "smart"`)
//!   reads as that setting's default, and a list entry it can't read is left out, so the unknown
//!   value costs that one setting, not the whole file.
//! - Writing: `merge` writes back the file as read with only what the setter changed, so keys,
//!   tables, list-entry keys and values this version doesn't know stay in the file.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use toml::{Table, Value};

/// A value that doesn't read as `T` reads as `T::default()`.
pub(crate) fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let value = Value::deserialize(deserializer)?;
    Ok(T::deserialize(value).unwrap_or_default())
}

/// A list without the entries that don't read as `T`.
pub(crate) fn readable_entries<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let values = Vec::<Value>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|value| T::deserialize(value).ok())
        .collect())
}

/// The file to write: `original` (the file as read) with the change from `known` (what this
/// version read of it, serialized) to `new` (the same after the setter). A key `known` has and
/// `new` hasn't is removed, a changed value replaced; whatever `known` lacks (unknown to this
/// version, or unreadable) stays as it is.
pub(crate) fn merge(mut original: Table, known: &Table, new: &Table) -> Table {
    original.retain(|key, _| new.contains_key(key) || !known.contains_key(key));
    for (key, new_value) in new {
        match (original.get_mut(key), known.get(key)) {
            (Some(slot), Some(known_value)) => {
                let original_value = std::mem::replace(slot, Value::Boolean(false));
                *slot = merge_value(original_value, known_value, new_value);
            }
            (Some(slot), None) => *slot = new_value.clone(),
            (None, _) => {
                original.insert(key.clone(), new_value.clone());
            }
        }
    }
    original
}

fn merge_value(original: Value, known: &Value, new: &Value) -> Value {
    if known == new {
        // Unchanged: keeps a value this version read as a default (`"smart"`) and any keys it
        // doesn't know below it.
        return original;
    }
    match (original, known, new) {
        (Value::Table(original), Value::Table(known), Value::Table(new)) => {
            Value::Table(merge(original, known, new))
        }
        (Value::Array(original), _, Value::Array(new)) => Value::Array(merge_list(original, new)),
        (_, _, new) => new.clone(),
    }
}

/// Each entry of `new`, as the `original` entry it is part of (the same entry with keys this
/// version doesn't know), found wherever it moved; an entry without one is written as is.
fn merge_list(mut original: Vec<Value>, new: &[Value]) -> Vec<Value> {
    new.iter()
        .map(|entry| {
            match original
                .iter()
                .position(|candidate| contains(candidate, entry))
            {
                Some(index) => original.remove(index),
                None => entry.clone(),
            }
        })
        .collect()
}

/// Whether `outer` is `inner`, or `inner` with more keys in some table.
fn contains(outer: &Value, inner: &Value) -> bool {
    match (outer, inner) {
        (Value::Table(outer), Value::Table(inner)) => inner.iter().all(|(key, inner_value)| {
            outer
                .get(key)
                .is_some_and(|outer_value| contains(outer_value, inner_value))
        }),
        (Value::Array(outer), Value::Array(inner)) => {
            outer.len() == inner.len() && outer.iter().zip(inner).all(|(o, i)| contains(o, i))
        }
        _ => outer == inner,
    }
}
