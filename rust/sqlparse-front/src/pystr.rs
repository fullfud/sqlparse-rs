//! Python 3.11 `str` methods the port needs, exactly: upper, lower (with final sigma), capitalize, the
//! whitespace set of strip/split, splitlines' line boundaries, and repr. Tables come from Python itself
//! (tools/gen_unicode.py), so the Unicode version is Python's, not Rust's.

use super::lexer::in_ranges;
use super::pyunicode_tables::{CASE_IGNORABLE, CASED, LOWER, NOT_PRINTABLE, TITLE, UPPER};

fn lookup(table: &'static [(u32, &'static str)], c: char) -> Option<&'static str> {
    table
        .binary_search_by_key(&(c as u32), |&(k, _)| k)
        .ok()
        .map(|i| table[i].1)
}

fn push_mapped(out: &mut String, table: &'static [(u32, &'static str)], c: char) {
    match lookup(table, c) {
        Some(m) => out.push_str(m),
        None => out.push(c),
    }
}

/// `s.upper()`.
pub(crate) fn upper(s: &str) -> String {
    if s.is_ascii() {
        return s.to_ascii_uppercase();
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        push_mapped(&mut out, UPPER, c);
    }
    out
}

/// CPython's handle_capital_sigma: U+03A3 at `i` of `chars` lowers to U+03C2 at the end of a word.
fn final_sigma(chars: &[char], i: usize) -> bool {
    let ignorable = |c: char| in_ranges(CASE_IGNORABLE, c);
    let cased = |c: char| in_ranges(CASED, c);
    let before = chars[..i].iter().rev().find(|&&c| !ignorable(c));
    if !before.is_some_and(|&c| cased(c)) {
        return false;
    }
    match chars[i + 1..].iter().find(|&&c| !ignorable(c)) {
        None => true,
        Some(&c) => !cased(c),
    }
}

fn lower_chars(chars: &[char], from: usize, out: &mut String) {
    for i in from..chars.len() {
        let c = chars[i];
        if c == '\u{3a3}' {
            out.push(if final_sigma(chars, i) {
                '\u{3c2}'
            } else {
                '\u{3c3}'
            });
        } else {
            push_mapped(out, LOWER, c);
        }
    }
}

/// `s.lower()`.
pub(crate) fn lower(s: &str) -> String {
    if s.is_ascii() {
        return s.to_ascii_lowercase();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    lower_chars(&chars, 0, &mut out);
    out
}

/// `s.capitalize()`: the first character in title case, the rest lowered.
pub(crate) fn capitalize(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    if let Some(&first) = chars.first() {
        push_mapped(&mut out, TITLE, first);
        lower_chars(&chars, 1, &mut out);
    }
    out
}

/// `c.isspace()` (29 characters: Unicode White_Space plus U+001C..U+001F).
pub(crate) fn isspace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\x0b'
            | '\x0c'
            | '\r'
            | '\x1c'..='\x1f'
            | ' '
            | '\u{85}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
    )
}

/// `s.strip()`.
pub(crate) fn strip(s: &str) -> &str {
    s.trim_matches(isspace)
}

/// `s.rstrip()`.
pub(crate) fn rstrip(s: &str) -> &str {
    s.trim_end_matches(isspace)
}

/// `len(s.splitlines())`.
pub(crate) fn splitlines_count(s: &str) -> usize {
    let mut n = 0;
    let mut line_open = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                n += 1;
                line_open = false;
            }
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => {
                n += 1;
                line_open = false;
            }
            _ => line_open = true,
        }
    }
    n + usize::from(line_open)
}

/// `repr(s)`.
pub(crate) fn repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c if c.is_ascii() => out.push(c),
            c if in_ranges(NOT_PRINTABLE, c) => {
                let v = c as u32;
                if v <= 0xff {
                    out.push_str(&format!("\\x{v:02x}"));
                } else if v <= 0xffff {
                    out.push_str(&format!("\\u{v:04x}"));
                } else {
                    out.push_str(&format!("\\U{v:08x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// `s.splitlines()`.
pub(crate) fn splitlines(s: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let end = match c {
            '\r' => {
                if let Some(&(_, '\n')) = it.peek() {
                    it.next();
                    i + 2
                } else {
                    i + 1
                }
            }
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => i + c.len_utf8(),
            _ => continue,
        };
        lines.push(&s[start..i]);
        start = end;
    }
    if start < s.len() {
        lines.push(&s[start..]);
    }
    lines
}

/// `len(s)`: code points.
pub(crate) fn len(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.chars().count()
    }
}
