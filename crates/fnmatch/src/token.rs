// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

//! This module parses an [fnmatch](https://man7.org/linux/man-pages/man3/fnmatch.3.html)
//! string pattern into a vector of [Token]s
//! (akin to regex that compiles a string pattern
//! into an automaton, but the solution used here is much simpler than that).
//! The way parsing works is the usual lexing + parsing sequence;
//! the lexing phase produces [RawToken]s.
//!
//! Transforming the string pattern into a representation of it
//! is not strictly necessary as far as matching is concerned,
//! but it's much easier to work with, especially because of
//! the grouping syntax that spans a few characters.
//!
//! As said, transforming the string is just convenience,
//! so it must be as fast as possible to not interfere with end user's
//! expectations. This module **never** allocates memory except for
//! `Vec<Token>`, and [Token] does not copy substrings of the original
//! pattern string. Unfortunately Rust does not allow self-referencing
//! struct fields, so [Token] has to store ranges instead of `&str`.
//! This is harder to work with, but there's no way around it.

// Keep this module private.

use std::ops::Range;

/// Components of an `fnmatch` string pattern.
/// A string pattern is composed of a vector of [Token]s.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Token {
    /// String literal to match.
    /// This token does not store the string itself, but
    /// it can be constructed by doing `&pattern[this_range]`.
    Text(Range<usize>),

    /// A wildcard that matches either a single or multiple characters,
    /// excluding the path separator.
    /// A wildcard may have a name, so that it is possible to create
    /// an associative map between the name and the value it resolved into.
    /// The wildcard name, when present, can be resolved into a string by doing
    /// `&pattern[this_range]`.
    Wildcard {
        name: Option<Range<usize>>,
        matcher: Matcher,
    },
}

/// Types of wildcards.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Matcher {
    /// Matches exactly one character, excluding the path separator.
    One,
    /// Matches zero or more characters, excluding the path separator.
    Any,
}

impl From<&RawToken> for Matcher {
    fn from(value: &RawToken) -> Self {
        match value {
            RawToken::MatchOne => Self::One,
            RawToken::MatchAny => Self::Any,
            // This is private API. If we reach this branch,
            // we have a bug and it's our fault.
            _ => unreachable!(),
        }
    }
}

/// Parses a string pattern into its components.
pub(crate) fn tokens(pattern: &str) -> Vec<Token> {
    let mut tokens = Vec::new();

    let mut raw_tokens = RawPattern::new(pattern);
    loop {
        let prev_index = raw_tokens.index;
        let Some(raw) = raw_tokens.next() else {
            break;
        };
        let curr_index = raw_tokens.index;

        match raw {
            RawToken::Escape => {
                let mut peek = raw_tokens;
                if let Some(next_tok) = peek.next()
                    && next_tok.is_escapable()
                {
                    append_text(&mut tokens, curr_index..peek.index);
                    skip_n(&mut raw_tokens, 1);
                    continue;
                }
                append_text(&mut tokens, prev_index..curr_index);
            }
            RawToken::GroupOpening => {
                if let Some((name, matcher)) = group_parameters(raw_tokens) {
                    tokens.push(Token::Wildcard {
                        name: Some(name),
                        matcher,
                    });
                    skip_n(&mut raw_tokens, 4);
                    continue;
                }
                append_text(&mut tokens, prev_index..curr_index);
            }
            RawToken::MatchOne | RawToken::MatchAny => {
                tokens.push(Token::Wildcard {
                    name: None,
                    matcher: Matcher::from(&raw),
                });
            }
            _ => append_text(&mut tokens, prev_index..curr_index),
        }
    }

    tokens
}

#[derive(Debug, PartialEq)]
enum RawToken {
    /// Escapes the character that follows.
    Escape,
    /// Any text.
    Text(Range<usize>),
    /// Opens the named wildcard.
    GroupOpening,
    /// Separates the wildcard name from the matcher type.
    GroupSeparator,
    /// Closes the named wildcard.
    GroupClosing,
    /// Matches any single character.
    MatchOne,
    /// Matches zero or more characters.
    MatchAny,
}

impl RawToken {
    /// Returns the non-textual raw token represented
    /// by `ch`. If `ch` is not a non-textual
    /// token, None is returned.
    fn nontext_from_char(ch: char) -> Option<Self> {
        Some(match ch {
            '\\' => Self::Escape,
            '(' => Self::GroupOpening,
            ':' => Self::GroupSeparator,
            ')' => Self::GroupClosing,
            '?' => Self::MatchOne,
            '*' => Self::MatchAny,
            _ => return None,
        })
    }

    /// Returns whether this raw token can be escapable.
    /// A token is escapable if it has special meaning in
    /// the `fnmatch` syntax.
    fn is_escapable(&self) -> bool {
        !matches!(self, Self::Text(_))
    }
}

/// Iterator over the raw tokens of a pattern string without allocating.
#[derive(Clone, Copy, Debug)]
struct RawPattern<'a> {
    pattern: &'a str,
    index: usize,
}

impl<'a> RawPattern<'a> {
    fn new(s: &'a str) -> Self {
        Self { pattern: s, index: 0 }
    }
}

impl<'a> Iterator for RawPattern<'a> {
    type Item = RawToken;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.pattern.len() {
            return None;
        }

        let mut txt_start = None;
        for (i, ch) in self.pattern[self.index..].char_indices() {
            let curr_index = self.index + i;
            if let Some(token) = RawToken::nontext_from_char(ch) {
                if let Some(txt_start) = txt_start {
                    self.index = curr_index;
                    return Some(RawToken::Text(txt_start..curr_index));
                }
                self.index = curr_index + ch.len_utf8();
                return Some(token);
            }
            if txt_start.is_none() {
                txt_start = Some(curr_index);
            }
        }

        if let Some(txt_start) = txt_start {
            self.index = self.pattern.len();
            return Some(RawToken::Text(txt_start..self.pattern.len()));
        }
        None
    }
}

/// Returns the name and the the matcher of a named wildcard.
/// If the named wildcard syntax is invalid, it returns None.
fn group_parameters<I>(mut raws: I) -> Option<(Range<usize>, Matcher)>
where
    I: Iterator<Item = RawToken> + Clone,
{
    let name = raws.next()?;
    let separator = raws.next()?;
    let matcher = raws.next()?;
    let closing = raws.next()?;

    if !matches!(separator, RawToken::GroupSeparator) || !matches!(closing, RawToken::GroupClosing) {
        return None;
    }

    let name = match name {
        RawToken::Text(name) => name,
        _ => return None,
    };

    let matcher = match matcher {
        RawToken::MatchOne => Matcher::One,
        RawToken::MatchAny => Matcher::Any,
        _ => return None,
    };

    Some((name, matcher))
}

/// Extends the previous text token when it 's present and is
/// contiguous to `range`, otherwise pushes a new text token
/// into the vector.
fn append_text(tokens: &mut Vec<Token>, range: Range<usize>) {
    if let Some(Token::Text(prev)) = tokens.last_mut()
        && prev.end == range.start
    {
        prev.end = range.end;
        return;
    }
    tokens.push(Token::Text(range));
}

/// Skips the next N elements in an iterator. Contrary to
/// [Iterator::skip], this function does not consume the iterator.
fn skip_n<I>(it: &mut impl Iterator<Item = I>, n: usize) {
    for _ in 0..n {
        it.next();
    }
}

#[cfg(test)]
mod raw_token_tests {
    use super::{RawPattern, RawToken};

    #[test]
    fn tokenize_only_text() {
        const PATH: &str = "/usr/bin/moss";
        let tokens: Vec<_> = RawPattern::new(PATH).collect();
        assert_eq!(tokens, vec![RawToken::Text(0..PATH.len())]);
    }

    #[test]
    fn tokenize_only_control_chars() {
        let tokens: Vec<_> = RawPattern::new("\\(:*?)").collect();
        assert_eq!(
            tokens,
            vec![
                RawToken::Escape,
                RawToken::GroupOpening,
                RawToken::GroupSeparator,
                RawToken::MatchAny,
                RawToken::MatchOne,
                RawToken::GroupClosing,
            ],
        );
    }

    #[test]
    fn tokenize_mixed_text() {
        let tokens: Vec<_> = RawPattern::new("/usr/(bindir:*)/moss").collect();
        assert_eq!(
            tokens,
            vec![
                RawToken::Text(0..5),
                RawToken::GroupOpening,
                RawToken::Text(6..12),
                RawToken::GroupSeparator,
                RawToken::MatchAny,
                RawToken::GroupClosing,
                RawToken::Text(15..20),
            ],
        );
    }
}

#[cfg(test)]
mod token_tests {
    use super::{Matcher, Token, tokens};

    #[test]
    fn tokenize_only_text() {
        let tokens = tokens("/usr/bin/moss");
        assert_eq!(tokens, vec![Token::Text(0..13)]);
    }

    #[test]
    fn tokenize_with_unnamed_middle_wildcard() {
        let tokens = tokens("/usr/*/moss");
        assert_eq!(
            tokens,
            vec![
                Token::Text(0..5),
                Token::Wildcard {
                    name: None,
                    matcher: Matcher::Any
                },
                Token::Text(6..11),
            ]
        );
    }

    #[test]
    fn tokenize_with_named_middle_() {
        let tokens = tokens("/usr/(bindir:*)/moss");
        assert_eq!(
            tokens,
            vec![
                Token::Text(0..5),
                Token::Wildcard {
                    name: Some(6..12),
                    matcher: Matcher::Any
                },
                Token::Text(15..20),
            ]
        );
    }

    #[test]
    fn tokenize_with_unnamed_trailing_wildcard() {
        let tokens = tokens("/usr/moss*");
        assert_eq!(
            tokens,
            vec![
                Token::Text(0..9),
                Token::Wildcard {
                    name: None,
                    matcher: Matcher::Any
                },
            ]
        );
    }

    #[test]
    fn tokenize_with_escaped_middle_wildcard() {
        let tokens = tokens(r"/usr/\*/moss");
        assert_eq!(
            tokens,
            vec![
                Token::Text(0..5),
                // The backslash is skipped.
                Token::Text(6..12)
            ]
        );
    }

    #[test]
    fn tokenize_with_incomplete_named_wildcard() {
        // Note the absence of the closing bracket.
        let tokens = tokens("(bindir:*");
        assert_eq!(
            tokens,
            vec![
                Token::Text(0..8),
                Token::Wildcard {
                    name: None,
                    matcher: Matcher::Any
                },
            ]
        );
    }

    #[test]
    fn tokenize_with_invalid_escape() {
        let tokens = tokens(r"/usr/\bin/moss\");
        assert_eq!(tokens, vec![Token::Text(0..15),]);
    }
}
