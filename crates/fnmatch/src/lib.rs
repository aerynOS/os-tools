// SPDX-FileCopyrightText: 2023 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

//! Implementation of an [`fnmatch`](https://man7.org/linux/man-pages/man3/fnmatch.3.html)-like
//! path matcher.
//!
//! It can match combinations of literal strings, the `*` (pronounced "star") wildcard
//! that matches zero or more characters, and the `?` wildcard that matches exactly one
//! character. The matcher is UTF-8-compatible, so here a character is really a
//! [Unicode scalar value](https://www.unicode.org/glossary/#unicode_scalar_value).
//! This module works on file paths, so the path separator is special and won't be matched
//! by wildcards.
//!
//! Additionally, the matcher allows named wildcards, that similarly to regex's capture
//! groups, associate a name to the substring the wildcard resolved into. Named groups
//! are represented by the following syntax: `(groupname:wildcard)`, where `groupname` is
//! a user-specified name and `wildcard` is one of the supported wildcards.
//!
//! The matching algorithm is the [*Sea of Stars*](https://git.musl-libc.org/cgit/musl/tree/src/regex/fnmatch.c).
//!
//! # Examples
//! ```
//!use fnmatch::Pattern;
//!
//!assert!(
//!    Pattern::new("/usr/bin/mkfs.*".to_string())
//!        .matches("/usr/bin/mkfs.ext4")
//!        .is_some()
//!);
//!
//! let matches = Pattern::new("/usr/bin/mkfs.(filesystem:*)".to_string()).matches("/usr/bin/mkfs.ext4");
//! assert!(matches.is_some_and(|m| m["filesystem"] == "ext4"));
//! ```

use std::{collections::HashMap, ops::Range, path::MAIN_SEPARATOR};

use serde_core::de;

use crate::token::{Matcher, Token, tokens};

mod token;

/// The path matcher. See [crate-level](crate)
/// documentation for usage.
#[derive(Clone, Debug)]
pub struct Pattern {
    pattern: String,
    tokens: Vec<Token>,
}

impl<S: Into<String>> From<S> for Pattern {
    fn from(value: S) -> Self {
        let pattern = value.into();
        let tokens = tokens(&pattern);
        Self { pattern, tokens }
    }
}

impl<'de> de::Deserialize<'de> for Pattern {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde_core::Deserializer<'de>,
    {
        Ok(String::deserialize(deserializer)?.into())
    }
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern
    }
}

impl Eq for Pattern {}

impl PartialOrd for Pattern {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Pattern {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.pattern.cmp(&other.pattern)
    }
}

impl Pattern {
    /// Creates a new Pattern.
    pub fn new(s: impl Into<String>) -> Self {
        s.into().into()
    }

    /// Tries to match the pattern against a given file path.
    /// If it matches, it returns the HashMap with the results of named wildcards.
    /// If no named wildcard was specified in the pattern string, the HashMap is empty.
    /// If the pattern doesn't match the path, None is returned.
    pub fn matches(&self, path: impl AsRef<str>) -> Option<HashMap<String, String>> {
        let mut path = path.as_ref();
        let mut matches = HashMap::new();

        let (head, tail, body) = head_tail_body(&self.tokens);
        if !self.match_head(&mut path, head, &mut matches) {
            return None;
        }
        if !self.match_tail(&mut path, tail.rev(), &mut matches) {
            return None;
        }
        if !self.match_body(&mut path, body, &mut matches) {
            return None;
        }
        Some(matches)
    }

    fn match_head<'a, I>(&self, walker: &mut &str, head: I, matches: &mut HashMap<String, String>) -> bool
    where
        I: Iterator<Item = &'a Token>,
    {
        for tok in head {
            match tok {
                Token::Text(txt) => {
                    if !walker.starts_with(&self.pattern[txt.clone()]) {
                        return false;
                    }
                    *walker = &walker[txt.len()..];
                }

                Token::Wildcard {
                    name,
                    matcher: Matcher::One,
                } => {
                    let Some(next_char) = walker.chars().next() else {
                        return false;
                    };
                    if next_char == MAIN_SEPARATOR {
                        return false;
                    }
                    self.save_group_match(name, matches, next_char);
                    *walker = &walker[next_char.len_utf8()..];
                }

                _ => unreachable!(),
            }
        }
        true
    }

    fn match_tail<'a, I>(&self, walker: &mut &str, head: I, matches: &mut HashMap<String, String>) -> bool
    where
        I: Iterator<Item = &'a Token>,
    {
        for tok in head {
            match tok {
                Token::Text(txt) => {
                    if !walker.ends_with(&self.pattern[txt.clone()]) {
                        return false;
                    }
                    *walker = &walker[..walker.len() - txt.len()];
                }

                Token::Wildcard {
                    name,
                    matcher: Matcher::One,
                } => {
                    let Some(next_char) = walker.chars().next_back() else {
                        return false;
                    };
                    if next_char == MAIN_SEPARATOR {
                        return false;
                    }
                    self.save_group_match(name, matches, next_char);
                    *walker = &walker[..walker.len() - next_char.len_utf8()];
                }

                _ => unreachable!(),
            }
        }
        true
    }

    fn match_body<'a, I>(&self, walker: &mut &str, body: I, matches: &mut HashMap<String, String>) -> bool
    where
        I: Iterator<Item = &'a Token>,
    {
        let mut glob_group_name = None;
        for tok in body {
            match tok {
                Token::Text(txt) => {
                    let Some(index) = walker.find(&self.pattern[txt.clone()]) else {
                        return false;
                    };
                    self.save_group_match(&glob_group_name, matches, &walker[..index]);
                    *walker = &walker[index + txt.len()..];
                }

                Token::Wildcard {
                    name,
                    matcher: Matcher::One,
                } => {
                    let Some(next_char) = walker.chars().next() else {
                        return false;
                    };
                    if next_char == MAIN_SEPARATOR {
                        return false;
                    }
                    self.save_group_match(name, matches, next_char);
                    *walker = &walker[next_char.len_utf8()..];
                }

                Token::Wildcard {
                    name,
                    matcher: Matcher::Any,
                    ..
                } => {
                    // In the body, an any-glob is basically a separator.
                    // As a side effect, consecutive any-globs are ignored
                    // and only the last one is considered.
                    glob_group_name = name.clone();
                }
            }
        }
        if *walker == "/" {
            return false;
        }
        self.save_group_match(&glob_group_name, matches, *walker);
        true
    }

    fn save_group_match(
        &self,
        name: &Option<Range<usize>>,
        matches: &mut HashMap<String, String>,
        value: impl Into<String>,
    ) {
        if let Some(group_name) = name {
            matches.insert(self.pattern[group_name.clone()].to_string(), value.into());
        }
    }
}

fn head_tail_body(
    tokens: &[Token],
) -> (
    impl Iterator<Item = &Token>,
    impl DoubleEndedIterator<Item = &Token>,
    impl Iterator<Item = &Token>,
) {
    fn is_glob(tok: &Token) -> bool {
        matches!(
            tok,
            Token::Wildcard {
                matcher: Matcher::Any,
                ..
            }
        )
    }

    let head = Range {
        start: 0,
        end: tokens.iter().position(is_glob).map_or(tokens.len(), |i| i.min(1)),
    };
    let tail = Range {
        start: tokens[head.end..]
            .iter()
            .rposition(is_glob)
            .map_or(tokens.len(), |i| head.end + i + 1),
        end: tokens.len(),
    };
    let body = Range {
        start: head.end,
        end: tail.start,
    };

    (tokens[head].iter(), tokens[tail].iter(), tokens[body].iter())
}
