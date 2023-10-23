use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

use crate::Timestamp;

impl TryFrom<Timestamp> for DateTime<Utc> {
    type Error = &'static str;

    fn try_from(value: Timestamp) -> Result<Self, Self::Error> {
        match DateTime::<Utc>::from_timestamp(value.seconds, value.nanos) {
            Some(datetime) => Ok(datetime),
            None => Err("Invalid or out-of-range timestamp"),
        }
    }
}

impl From<DateTime<Utc>> for Timestamp {
    fn from(dt: DateTime<Utc>) -> Self {
        Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos(),
        }
    }
}

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let time_stamp = Timestamp {
            seconds: self.seconds,
            nanos: self.nanos,
        };
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
