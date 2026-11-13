// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{KeyValue, sequence_of_key_value, serde_util::parse_bool};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoolOrMap {
    /// Apply a global default
    All(bool),
    /// Per-key overrides
    Map(Vec<KeyValue<bool>>),
}

impl BoolOrMap {
    pub fn resolve(&self, default: bool, matches: impl Fn(&str) -> bool) -> bool {
        match self {
            Self::All(enabled) => *enabled,
            Self::Map(overrides) => overrides
                .iter()
                .rev()
                .find(|entry| matches(&entry.key))
                .map(|entry| entry.value)
                .unwrap_or(default),
        }
    }
}

pub fn default_enabled() -> BoolOrMap {
    BoolOrMap::All(true)
}

pub fn default_disabled() -> BoolOrMap {
    BoolOrMap::All(false)
}

impl<'de> Deserialize<'de> for BoolOrMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Inner {
            Bool(bool),
            String(String),
            Sequence(Vec<BTreeMap<String, bool>>),
        }

        match Inner::deserialize(deserializer)? {
            Inner::Bool(value) => Ok(BoolOrMap::All(value)),
            Inner::String(value) => Ok(BoolOrMap::All(parse_bool(&value).map_err(serde::de::Error::custom)?)),
            Inner::Sequence(sequence) => {
                let mut overrides = vec![];

                for map in sequence {
                    if map.is_empty() {
                        return Err(serde::de::Error::custom("missing entry"));
                    }

                    overrides.extend(map.into_iter().map(|(key, value)| KeyValue { key, value }));
                }

                Ok(BoolOrMap::Map(overrides))
            }
        }
    }
}
