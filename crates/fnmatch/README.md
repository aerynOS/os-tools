<!--
# SPDX-FileCopyrightText: 2026 AerynOS Developers
# SPDX-License-Identifier: MPL-2.0
-->

# fnmatch

One way to implement the star (`*`) wildcard is to use backtracking.
Basically, the target string is walked left to right and when a star is encountered,
the matches recursively tries to resolve the remaining ("unwalked") string. If it fails,
it *tracks back*. Doing so is slow because of the many attempts, especially when the code needs
to heap allocate stuff.

Another approach is the Sea of Stars. The algorithm works like this:
1. The pattern is split in 3 portions: head, body, and tail.
   Head and tail are the beginning and end of the string, up to the next star wildcard.
   Body is the substring enclosed in the first and last star wildcards. E.g. for the `/usr/*/mkfs.ext*` pattern:
   - Head: `/usr/`
   - Body: `*/mkfs.ext*`
   - Tail: *empty*

   There will always be a head, but body and tail are optional, depending
   on how many stars are in the pattern.
1. Head and tail never contain star wildcards, so they are easy to compare against a target string.
The tail will be matched walking backwards from the end.
1. If both head and tail matched, it's the body's turn, that will be matched against what's left to walk of the target string. There's no need to guess anything, because all we got the body is a sequence of "star|text|star|text|...|star", so the star becomes a mere separator.
