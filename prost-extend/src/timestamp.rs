//! Implementation from prost-wkt crate.

use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

use crate::Timestamp;

const NANOS_PER_SECOND: i32 = 1_000_000_000;
//const NANOS_MAX: i32 = NANOS_PER_SECOND - 1;

impl Timestamp {
    /// Normalizes the timestamp to a canonical format.
    ///
    /// Based on [`google::protobuf::util::CreateNormalized`][1].
    ///
    /// [1]: https://github.com/google/protobuf/blob/v3.3.2/src/google/protobuf/util/time_util.cc#L59-L77
    pub fn normalize(&mut self) {
        // Make sure nanos is in the range.
        if self.nanos <= -NANOS_PER_SECOND || self.nanos >= NANOS_PER_SECOND {
            if let Some(seconds) = self
                .seconds
                .checked_add((self.nanos / NANOS_PER_SECOND) as i64)
            {
                self.seconds = seconds;
                self.nanos %= NANOS_PER_SECOND;
            } else if self.nanos < 0 {
                // Negative overflow! Set to the earliest normal value.
                self.seconds = i64::MIN;
                self.nanos = 0;
            } else {
                // Positive overflow! Set to the latest normal value.
                self.seconds = i64::MAX;
                self.nanos = 999_999_999;
            }
        }

        // For Timestamp nanos should be in the range [0, 999999999].
        if self.nanos < 0 {
            if let Some(seconds) = self.seconds.checked_sub(1) {
                self.seconds = seconds;
                self.nanos += NANOS_PER_SECOND;
            } else {
                // Negative overflow! Set to the earliest normal value.
                debug_assert_eq!(self.seconds, i64::MIN);
                self.nanos = 0;
            }
        }

        // TODO: should this be checked?
        // debug_assert!(self.seconds >= -62_135_596_800 && self.seconds <= 253_402_300_799,
        //               "invalid timestamp: {:?}", self);
    }
}

impl TryFrom<Timestamp> for DateTime<Utc> {
    type Error = &'static str;

    fn try_from(value: Timestamp) -> Result<Self, Self::Error> {
        let mut value = value;
        // A call to `normalize` should capture all out-of-bound situations hopefully
        // ensuring a panic never happens! Ideally this implementation should be
        // deprecated in favour of TryFrom but unfortunately having `TryFrom` along with
        // `From` causes a conflict.
        value.normalize();
        match DateTime::<Utc>::from_timestamp(value.seconds, value.nanos as u32) {
            Some(datetime) => Ok(datetime),
            None => Err("Invalid or out-of-range timestamp"),
        }
    }
}

impl From<DateTime<Utc>> for Timestamp {
    fn from(dt: DateTime<Utc>) -> Self {
        Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos() as i32,
        }
    }
}

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let mut time_stamp = Timestamp {
            seconds: self.seconds,
            nanos: self.nanos,
        };
        time_stamp.normalize();
        let datetime = DateTime::<Utc>::try_from(time_stamp).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&format!("{datetime:?}"))
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        struct TimestampVisitor;

        impl<'de> Visitor<'de> for TimestampVisitor {
            type Value = Timestamp;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "a timestamp in RFC3339 format")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let utc_datetime = DateTime::<Utc>::from_str(value).map_err(|error| {
                    serde::de::Error::custom(format!(
                        "Failed to parse {value} as datetime: {error:?}"
                    ))
                })?;
                Ok(Timestamp::from(utc_datetime))
            }
        }
        deserializer.deserialize_str(TimestampVisitor)
    }
}
