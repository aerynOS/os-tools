// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use serde::Deserialize;
use snafu::Snafu;

pub fn default_true() -> bool {
    true
}

#[derive(Debug, Snafu)]
pub enum BoolError {
    #[snafu(display("invalid boolean: expected true or false"))]
    Invalid,
}

pub fn parse_bool(value: &str) -> Result<bool, BoolError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(BoolError::Invalid),
    }
}

pub fn stringy_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Inner {
        Bool(bool),
        String(String),
    }

    match Inner::deserialize(deserializer)? {
        Inner::Bool(value) => Ok(value),
        Inner::String(value) => parse_bool(&value).map_err(serde::de::Error::custom),
    }
}
