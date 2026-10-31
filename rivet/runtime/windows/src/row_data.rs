//! The data form of a supported-build row, and the signed table that carries such rows.
//!
//! A row is a tree of plain values, so the same row can live in this binary or arrive in a signed
//! file without reinstalling Rivet. Every row struct declares itself through `row_struct!`, which
//! keeps its reader and writer complete: a field added to a struct is carried with no other change,
//! and a row that lacks it is refused rather than filled with a guess.
//!
//! The written form tells the row generator what each value is. A `usize` is a `0x` string, because
//! only addresses, offsets and slots have that type. Colours and constants are plain numbers and
//! instruction bytes are bare hex, so neither can be taken for an address.

use serde_json::{json, Map, Value};

/// Why a row was refused, with the path of the value that was wrong.
pub type RowResult<T> = Result<T, String>;

pub trait RowValue: Sized {
    fn to_row(&self) -> Value;
    fn from_row(value: &Value) -> RowResult<Self>;
}

impl RowValue for usize {
    fn to_row(&self) -> Value {
        Value::String(format!("{self:#x}"))
    }
    fn from_row(value: &Value) -> RowResult<Self> {
        value
            .as_str()
            .and_then(|text| text.strip_prefix("0x"))
            .and_then(|digits| usize::from_str_radix(digits, 16).ok())
            .ok_or_else(|| format!("{value} is not an address"))
    }
}

macro_rules! row_number {
    ($($type:ty),*) => {$(
        impl RowValue for $type {
            fn to_row(&self) -> Value {
                json!(*self)
            }
            fn from_row(value: &Value) -> RowResult<Self> {
                value
                    .as_i64()
                    .and_then(|number| <$type>::try_from(number).ok())
                    .ok_or_else(|| format!("{value} is not a {}", stringify!($type)))
            }
        }
    )*};
}
row_number!(i32, u32, u16);

impl RowValue for bool {
    fn to_row(&self) -> Value {
        Value::Bool(*self)
    }
    fn from_row(value: &Value) -> RowResult<Self> {
        value.as_bool().ok_or_else(|| format!("{value} is not a flag"))
    }
}

impl RowValue for &'static str {
    fn to_row(&self) -> Value {
        Value::String((*self).to_owned())
    }
    fn from_row(value: &Value) -> RowResult<Self> {
        let text = value.as_str().ok_or_else(|| format!("{value} is not text"))?;
        Ok(Box::leak(text.to_owned().into_boxed_str()))
    }
}

impl RowValue for &'static [u8] {
    fn to_row(&self) -> Value {
        Value::String(self.iter().map(|byte| format!("{byte:02x}")).collect())
    }
    fn from_row(value: &Value) -> RowResult<Self> {
        let text = value.as_str().ok_or_else(|| format!("{value} is not bytes"))?;
        let bytes = rivet_platform_windows::chrome_build::hex(text)
            .ok_or_else(|| format!("{value} is not hex"))?;
        Ok(Box::leak(bytes.into_boxed_slice()))
    }
}

impl<T: RowValue + 'static> RowValue for &'static [T] {
    fn to_row(&self) -> Value {
        Value::Array(self.iter().map(RowValue::to_row).collect())
    }
    fn from_row(value: &Value) -> RowResult<Self> {
        let items = value.as_array().ok_or_else(|| format!("{value} is not a list"))?;
        let items = items.iter().map(T::from_row).collect::<RowResult<Vec<_>>>()?;
        Ok(Box::leak(items.into_boxed_slice()))
    }
}

impl<T: RowValue> RowValue for Option<T> {
    fn to_row(&self) -> Value {
        self.as_ref().map_or(Value::Null, RowValue::to_row)
    }
    fn from_row(value: &Value) -> RowResult<Self> {
        if value.is_null() {
            Ok(None)
        } else {
            T::from_row(value).map(Some)
        }
    }
}

impl<T: RowValue, const N: usize> RowValue for [T; N] {
    fn to_row(&self) -> Value {
        Value::Array(self.iter().map(RowValue::to_row).collect())
    }
    fn from_row(value: &Value) -> RowResult<Self> {
        let items = value.as_array().ok_or_else(|| format!("{value} is not a list"))?;
        let items = items.iter().map(T::from_row).collect::<RowResult<Vec<_>>>()?;
        items.try_into().map_err(|_| format!("{value} does not hold {N} values"))
    }
}

macro_rules! row_tuple {
    ($($type:ident $index:tt),*) => {
        impl<$($type: RowValue),*> RowValue for ($($type,)*) {
            fn to_row(&self) -> Value {
                Value::Array(vec![$(self.$index.to_row()),*])
            }
            fn from_row(value: &Value) -> RowResult<Self> {
                let items = value.as_array().ok_or_else(|| format!("{value} is not a pair"))?;
                if items.len() != [$($index),*].len() {
                    return Err(format!("{value} has the wrong length"));
                }
                Ok(($($type::from_row(&items[$index])?,)*))
            }
        }
    };
}
row_tuple!(A 0, B 1);
row_tuple!(A 0, B 1, C 2);

/// Reads one named field of a row object, naming it in any error.
pub fn field<T: RowValue>(object: &Map<String, Value>, name: &str) -> RowResult<T> {
    let value = object.get(name).ok_or_else(|| format!("{name} is missing"))?;
    T::from_row(value).map_err(|error| format!("{name}: {error}"))
}

/// Declares a row struct, and how it becomes a row value and back.
macro_rules! row_struct {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $($(#[$field_meta:meta])* pub $field:ident: $type:ty,)*
        }
    ) => {
        $(#[$meta])*
        pub struct $name {
            $($(#[$field_meta])* pub $field: $type,)*
        }

        impl $crate::row_data::RowValue for $name {
            fn to_row(&self) -> serde_json::Value {
                let mut object = serde_json::Map::new();
                $(object.insert(
                    stringify!($field).to_owned(),
                    $crate::row_data::RowValue::to_row(&self.$field),
                );)*
                serde_json::Value::Object(object)
            }
            fn from_row(value: &serde_json::Value) -> $crate::row_data::RowResult<Self> {
                let object = value
                    .as_object()
                    .ok_or_else(|| format!("{} is not an object", stringify!($name)))?;
                Ok(Self {
                    $($field: $crate::row_data::field(object, stringify!($field))?,)*
                })
            }
        }
    };
}
pub(crate) use row_struct;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builds;

    #[test]
    fn nfr6_every_compiled_row_survives_the_data_form() {
        for build in builds::compiled() {
            let row = build.to_row();
            let read = builds::Build::from_row(&row).unwrap_or_else(|error| {
                panic!("{} {:?}: {error}", build.browser, build.version)
            });
            assert_eq!(read.to_row(), row, "{} {:?}", build.browser, build.version);
            assert_eq!(read.version, build.version);
            assert_eq!(read.toolbar.layout_constant, build.toolbar.layout_constant);
            assert_eq!(read.toolbar.overrides, build.toolbar.overrides);
            assert_eq!(read.toolbar.colours, build.toolbar.colours);
            assert_eq!(read.toolbar.layout_constant_prologue, build.toolbar.layout_constant_prologue);
        }
    }

    #[test]
    fn nfr6_a_row_without_a_field_is_refused_and_named() {
        let mut row = builds::CHROME.to_row();
        row["toolbar"].as_object_mut().unwrap().remove("paint_slot");
        let error = match builds::Build::from_row(&row) {
            Err(error) => error,
            Ok(_) => panic!("a row with no paint slot was accepted"),
        };
        assert_eq!(error, "toolbar: paint_slot is missing");
    }

    #[test]
    fn nfr6_a_field_this_runtime_does_not_know_is_ignored() {
        // A table written for a newer runtime still serves this one.
        let mut row = builds::CHROME.to_row();
        row["toolbar"]["added_later"] = json!("0x1234");
        assert!(builds::Build::from_row(&row).is_ok());
    }

    #[test]
    fn nfr6_written_values_tell_addresses_from_colours_and_bytes() {
        let row = builds::CHROME.to_row();
        assert_eq!(row["toolbar"]["layout_constant"].as_str().map(|text| &text[..2]), Some("0x"));
        assert!(row["toolbar"]["colours"][0][1].is_number());
        let bytes = row["toolbar"]["layout_constant_prologue"].as_str().unwrap();
        assert!(!bytes.starts_with("0x") && bytes.len() % 2 == 0);
        assert!(row["version"][0].is_number());
    }

    #[test]
    fn nfr6_location_bar_values_travel_with_their_row() {
        assert!(builds::location_bar_minimum(&builds::CHROME_156_74).is_some());
        assert!(builds::location_bar_minimum(&builds::BRAVE_63).is_some());
        assert!(builds::location_bar_minimum(&builds::CHROME).is_none());
        let read = builds::Build::from_row(&builds::BRAVE_63.to_row()).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(read.location_bar.map(|bar| bar.function), Some(0x038E_1210));
    }
}
