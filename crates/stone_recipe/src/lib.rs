// SPDX-FileCopyrightText: 2023 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeMap;

use serde::Deserialize;
pub use serde_yaml::Error;

use crate::bool_or_map::BoolOrMap;
use crate::serde_util::{default_true, stringy_bool};

pub use self::control_file::ControlFile;
pub use self::macros::Macros;
pub use self::tuning::Tuning;
pub use self::upstream::Upstream;

pub mod bool_or_map;
pub mod control_file;
pub mod macros;
pub mod tuning;
pub mod upstream;

mod serde_util;

pub fn from_slice(bytes: &[u8]) -> Result<Recipe, Error> {
    serde_yaml::from_slice(bytes)
}

pub fn from_str(s: &str) -> Result<Recipe, Error> {
    serde_yaml::from_str(s)
}

#[derive(Debug, Clone, Deserialize)]
pub struct Recipe {
    #[serde(flatten)]
    pub source: Source,
    #[serde(flatten)]
    pub build: Build,
    #[serde(flatten)]
    pub package: Package,
    #[serde(flatten)]
    pub options: Options,
    #[serde(default, deserialize_with = "sequence_of_key_value")]
    pub profiles: Vec<KeyValue<Build>>,
    #[serde(default, rename = "packages", deserialize_with = "sequence_of_key_value")]
    pub sub_packages: Vec<KeyValue<Package>>,
    #[serde(default)]
    pub upstreams: Vec<Upstream>,
    #[serde(default)]
    pub architectures: Vec<String>,
    #[serde(default)]
    pub tuning: Vec<KeyValue<Tuning>>,
    #[serde(default, deserialize_with = "stringy_bool")]
    pub emul32: bool,
    #[serde(default, deserialize_with = "stringy_bool")]
    pub mold: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct KeyValue<T> {
    pub key: String,
    pub value: T,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    pub name: String,
    #[serde(deserialize_with = "force_string")]
    pub version: String,
    pub release: u64,
    pub homepage: String,
    #[serde(deserialize_with = "single_as_sequence")]
    pub license: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Build {
    pub setup: Option<String>,
    pub build: Option<String>,
    pub install: Option<String>,
    pub check: Option<String>,
    pub workload: Option<String>,
    pub environment: Option<String>,
    #[serde(default, rename = "builddeps")]
    pub build_deps: Vec<String>,
    #[serde(default, rename = "checkdeps")]
    pub check_deps: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Options {
    #[serde(default)]
    pub toolchain: tuning::Toolchain,
    #[serde(default, deserialize_with = "stringy_bool")]
    pub cspgo: bool,
    #[serde(default, deserialize_with = "stringy_bool")]
    pub samplepgo: bool,
    #[serde(default = "bool_or_map::default_enabled")]
    pub debug: BoolOrMap,
    #[serde(default = "bool_or_map::default_enabled")]
    pub strip: BoolOrMap,
    #[serde(default, deserialize_with = "stringy_bool")]
    pub networking: bool,
    #[serde(default, deserialize_with = "stringy_bool")]
    pub compressman: bool,
    #[serde(default = "default_true", deserialize_with = "stringy_bool")]
    pub lastrip: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Package {
    pub summary: Option<String>,
    pub description: Option<String>,
    #[serde(default, rename = "provides-exclude")]
    pub provides_exclude: Vec<String>,
    #[serde(default, rename = "rundeps")]
    pub run_deps: Vec<String>,
    #[serde(default, rename = "rundeps-exclude")]
    pub run_deps_exclude: Vec<String>,
    #[serde(default)]
    pub paths: Vec<Path>,
    #[serde(default)]
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub path: String,
    pub kind: PathKind,
}

impl<'de> Deserialize<'de> for Path {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Inner {
            String(String),
            KeyValue(BTreeMap<String, PathKind>),
        }

        match Inner::deserialize(deserializer)? {
            Inner::String(path) => Ok(Path {
                path,
                kind: PathKind::default(),
            }),
            Inner::KeyValue(map) => {
                if let Some((path, kind)) = map.into_iter().next() {
                    Ok(Path { path, kind })
                } else {
                    Err(serde::de::Error::custom("missing path entry"))
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, strum::EnumString, Default)]
#[serde(try_from = "&str")]
#[strum(serialize_all = "lowercase")]
pub enum PathKind {
    #[default]
    Any,
    Exe,
    Symlink,
    Special,
}

/// Deserialize a single value or sequence of values as a vec
fn single_as_sequence<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum Value<T> {
        Single(T),
        Sequence(Vec<T>),
    }

    match Value::deserialize(deserializer)? {
        Value::Single(value) => Ok(vec![value]),
        Value::Sequence(sequence) => Ok(sequence),
    }
}

/// Deserialize a single map or a sequence of maps as a vec of [`KeyValue`]
fn sequence_of_key_value<'de, T, D>(deserializer: D) -> Result<Vec<KeyValue<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    let entries = single_as_sequence::<BTreeMap<String, T>, D>(deserializer)?;

    Ok(entries
        .into_iter()
        .flat_map(|entry| entry.into_iter().map(|(key, value)| KeyValue { key, value }))
        .collect())
}

fn force_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Inner {
        String(String),
        Number(serde_yaml::Number),
    }

    match Inner::deserialize(deserializer)? {
        Inner::String(s) => Ok(s),
        Inner::Number(n) => Ok(n.to_string()),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn deserialize() {
        let inputs = [
            &include_bytes!("../../../test/llvm-stone.yml")[..],
            &include_bytes!("../../../test/boulder-stone.yml")[..],
        ];

        for input in inputs {
            let recipe = from_slice(input).unwrap();
            dbg!(&recipe);
        }
    }

    #[test]
    fn key_value_sequence_or_map() {
        #[derive(serde::Deserialize)]
        struct Test {
            #[serde(deserialize_with = "sequence_of_key_value")]
            entries: Vec<KeyValue<bool>>,
        }

        let single: Test = serde_yaml::from_str(
            "\
entries:
  /usr/bin/foo: false
",
        )
        .unwrap();
        assert_eq!(
            single.entries,
            vec![KeyValue {
                key: "/usr/bin/foo".to_owned(),
                value: false,
            }]
        );

        let sequence: Test = serde_yaml::from_str(
            "\
entries:
  - /usr/bin/foo: false
  - /usr/bin/bar: true
",
        )
        .unwrap();
        assert_eq!(
            sequence.entries,
            vec![
                KeyValue {
                    key: "/usr/bin/foo".to_owned(),
                    value: false,
                },
                KeyValue {
                    key: "/usr/bin/bar".to_owned(),
                    value: true,
                },
            ]
        );
    }

    #[test]
    fn options() {
        let default = "\
name: test
version: \"1.0\"
release: 1
homepage: https://example.com
license: MPL-2.0
";
        let disabled = "\
name: test
version: \"1.0\"
release: 1
homepage: https://example.com
license: MPL-2.0
strip: false
";
        let overridden = "\
name: test
version: \"1.0\"
release: 1
homepage: https://example.com
license: MPL-2.0
strip:
  - /usr/bin/foo: false
";

        assert_eq!(from_str(default).unwrap().options.strip, BoolOrMap::All(true));
        assert_eq!(from_str(disabled).unwrap().options.strip, BoolOrMap::All(false));
        assert_eq!(
            from_str(overridden).unwrap().options.strip,
            BoolOrMap::Map(vec![KeyValue {
                key: "/usr/bin/foo".to_owned(),
                value: false,
            }])
        );
    }
}
