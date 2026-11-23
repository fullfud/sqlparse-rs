//! Port of `sqlparse/lexer.py` and `keywords.find_delimited_spans` (sqlparse 0.6.1).
//!
//! Positions are byte offsets into the text; sqlparse counts code points, but every position here is only
//! compared and sliced, so the tokens come out the same. Rules are tried in SQL_REGEX order at each position,
//! anchored there, with sqlparse's one-character lookbehinds checked on the character before.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::lexer_tables::{Action, Before, DOLLAR_DELIM, DOLLAR_OPENER_NOT_AFTER, KEYWORDS, RULES};

const COMMENT_TAG: &str = "/*";

struct Compiled {
    re: fancy_regex::Regex,
    /// Run first, on the regex crate's engine: no match there means no match of `re` (see gen_lexer.py).
    prefilter: Option<regex::Regex>,
    before: &'static Before,
    first: &'static [(u32, u32)],
    action: &'static Action,
}

struct Rules {
    all: Vec<Compiled>,
    /// For each ASCII character, the rules (in SQL_REGEX order) a match can start with it.
    ascii: Vec<Vec<usize>>,
}

impl Rules {
    /// The rules worth trying at a position starting with `c`, in order: the others cannot match there.
    fn candidates(&self, c: char) -> impl Iterator<Item = &Compiled> {
        let (ascii, other) = if c.is_ascii() {
            (
                Some(self.ascii[c as usize].iter().map(|&i| &self.all[i])),
                None,
            )
        } else {
            (
                None,
                Some(self.all.iter().filter(move |r| in_ranges(r.first, c))),
            )
        };
        ascii
            .into_iter()
            .flatten()
            .chain(other.into_iter().flatten())
    }
}

fn rules() -> &'static Rules {
    static RULES_RE: OnceLock<Rules> = OnceLock::new();
    RULES_RE.get_or_init(|| {
        let all: Vec<Compiled> = RULES
            .iter()
            .map(|r| Compiled {
                re: fancy_regex::Regex::new(&format!("^(?:{})", r.pattern))
                    .unwrap_or_else(|e| panic!("lexer rule {:?}: {e}", r.pattern)),
                prefilter: r.prefilter.map(|p| {
                    regex::Regex::new(&format!("^(?:{p})"))
                        .unwrap_or_else(|e| panic!("lexer prefilter {p:?}: {e}"))
                }),
                before: &r.before,
                first: r.first,
                action: &r.action,
            })
            .collect();
        let ascii = (0u8..128)
            .map(|b| {
                (0..all.len())
                    .filter(|&i| in_ranges(all[i].first, char::from(b)))
                    .collect()
            })
            .collect();
        Rules { all, ascii }
    })
}

fn dollar_delim() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(&format!("^(?:{DOLLAR_DELIM})"))
            .unwrap_or_else(|e| panic!("dollar delimiter: {e}"))
    })
}

pub(crate) fn in_ranges(ranges: &[(u32, u32)], c: char) -> bool {
    let c = c as u32;
    ranges
        .binary_search_by(|&(a, b)| {
            if b < c {
                std::cmp::Ordering::Less
            } else if a > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// The keyword table as an open-addressing hash table of indices into KEYWORDS (u16::MAX: empty), built once.
/// A lookup hashes the upper-cased word and compares one or two entries, where a binary search over the 800
/// keywords compared about ten strings.
struct KeywordIndex {
    slots: Vec<u16>,
    mask: usize,
}

fn word_hash(word: &[u8]) -> usize {
    // FNV-1a
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in word {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    (h ^ (h >> 29)) as usize
}

fn keyword_index() -> &'static KeywordIndex {
    static INDEX: std::sync::OnceLock<KeywordIndex> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| {
        let size = (KEYWORDS.len() * 4).next_power_of_two();
        let mut slots = vec![u16::MAX; size];
        for (i, (k, _)) in KEYWORDS.iter().enumerate() {
            let mut at = word_hash(k.as_bytes()) & (size - 1);
            while slots[at] != u16::MAX {
                at = (at + 1) & (size - 1);
            }
            slots[at] = i as u16;
        }
        KeywordIndex {
            slots,
            mask: size - 1,
        }
    })
}

/// `Lexer.is_keyword`: the keyword's token type, or `Token.Name`.
fn keyword_type(value: &str) -> &'static str {
    let find = |up: &[u8]| {
        let index = keyword_index();
        let mut at = word_hash(up) & index.mask;
        loop {
            match index.slots[at] {
                u16::MAX => return "Token.Name",
                i if KEYWORDS[i as usize].0.as_bytes() == up => return KEYWORDS[i as usize].1,
                _ => at = (at + 1) & index.mask,
            }
        }
    };
    // Nearly every word is short ASCII: upper-case it on the stack instead of in a new String.
    let mut buf = [0u8; 32];
    if value.len() <= buf.len() && value.is_ascii() {
        let up = &mut buf[..value.len()];
        up.copy_from_slice(value.as_bytes());
        up.make_ascii_uppercase();
        return find(up);
    }
    find(super::pystr::upper(value).as_bytes())
}

/// `keywords.DelimitedSpans`: openers by start, closers by tag.
struct Spans<'a> {
    openers: HashMap<usize, (usize, &'a str, &'static str)>,
    closers: HashMap<&'a str, (Vec<usize>, Vec<usize>)>,
}

impl Spans<'_> {
    fn resolve(&self, pos: usize) -> Option<(usize, &'static str)> {
        if self.openers.is_empty() {
            return None;
        }
        let &(opener_end, tag, ttype) = self.openers.get(&pos)?;
        let (starts, ends) = self.closers.get(tag)?;
        let idx = starts.partition_point(|&s| s < opener_end);
        ends.get(idx).map(|&end| (end, ttype))
    }
}

/// `keywords.find_delimited_spans`.
fn find_delimited_spans(text: &str) -> Spans<'_> {
    let mut spans = Spans {
        openers: HashMap::new(),
        closers: HashMap::new(),
    };
    let has_dollar = text.contains('$');
    let has_comment_open = text.contains("/*");
    if has_dollar {
        for (start, _) in text.match_indices('$') {
            let Some(m) = dollar_delim().find(&text[start..]) else {
                continue;
            };
            let end = start + m.end();
            let tag = &text[start..end];
            let (starts, ends) = spans.closers.entry(tag).or_default();
            starts.push(start);
            ends.push(end);
            let opens = text[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !in_ranges(DOLLAR_OPENER_NOT_AFTER, c));
            if opens {
                spans.openers.insert(start, (end, tag, "Token.Literal"));
            }
        }
    }
    if has_comment_open {
        for (start, _) in text.match_indices("/*+") {
            spans.openers.insert(
                start,
                (start + 3, COMMENT_TAG, "Token.Comment.Multiline.Hint"),
            );
        }
        for (start, _) in text.match_indices("/*") {
            if text[start + 2..].starts_with('+') {
                continue;
            }
            spans
                .openers
                .insert(start, (start + 2, COMMENT_TAG, "Token.Comment.Multiline"));
        }
        let (starts, ends) = spans.closers.entry(COMMENT_TAG).or_default();
        for (start, _) in text.match_indices("*/") {
            starts.push(start);
            ends.push(start + 2);
        }
    }
    spans
}

const WHITESPACE: &str = "Token.Text.Whitespace";
const NEWLINE: &str = "Token.Text.Whitespace.Newline";
const PUNCTUATION: &str = "Token.Punctuation";
const COMPARISON: &str = "Token.Operator.Comparison";
const NAME: &str = "Token.Name";
const KEYWORD: &str = "Token.Keyword";
const STRING: &str = "Token.Literal.String.Single";
const HEXADECIMAL: &str = "Token.Literal.Number.Hexadecimal";
const FLOAT: &str = "Token.Literal.Number.Float";
const INTEGER: &str = "Token.Literal.Number.Integer";

/// What the fast path found at a position.
enum Fast {
    /// The token: its type and length in bytes.
    Token(&'static str, usize),
    /// A word that may start a multi-word keyword: only these rules can match there, and if none does, the
    /// word itself (this many bytes) is rule 45's.
    Phrase(&'static [usize], usize),
    /// Only the rules can tell.
    Rules,
}

/// Rule 13, `(CASE|IN|VALUES|USING|FROM|AS)\b`: a whole word, any case.
const RULE_13_WORDS: [&[u8]; 6] = [b"AS", b"CASE", b"FROM", b"IN", b"USING", b"VALUES"];

/// The multi-word keyword rules (26-44: `LEFT\s+...JOIN\b`, `GROUP\s+BY\b`, `WITH'\s+TIME`, ...) a word can
/// start: each of those rules starts with one of these words followed by whitespace, `\b` or `'`, so it cannot
/// match from any other word (the `phrase_rules_by_start_word` test).
fn phrase_rules(upper: &[u8]) -> Option<&'static [usize]> {
    Some(match upper {
        b"LEFT" | b"RIGHT" | b"FULL" | b"INNER" | b"OUTER" | b"STRAIGHT" | b"CROSS"
        | b"NATURAL" | b"JOIN" => &[26],
        b"END" => &[27],
        b"IF" => &[28],
        b"NOT" => &[29, 43, 44],
        b"ASC" | b"DESC" => &[30, 31],
        b"NULLS" => &[32],
        b"UNION" => &[33],
        b"CREATE" => &[34],
        b"DOUBLE" => &[35],
        b"GROUP" => &[36],
        b"ORDER" => &[37],
        b"PRIMARY" => &[38],
        b"HANDLER" => &[39],
        b"GO" => &[40],
        b"LATERAL" => &[41],
        b"AT" | b"WITH" => &[42],
        b"LIKE" | b"ILIKE" | b"RLIKE" => &[43],
        b"REGEXP" => &[44],
        _ => return None,
    })
}

/// Python's `\s` among ASCII characters (str patterns): `\t\n\x0b\x0c\r`, `\x1c`-`\x1f` and the space.
fn is_py_space(b: u8) -> bool {
    matches!(b, b'\t'..=b'\r' | 0x1c..=b' ')
}

fn is_ascii_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The token at `pos` without the regexes, for the commonest ASCII cases, or None where only the rules can
/// tell. Each case takes the rules that can start with that character (`Rules::ascii`, pinned by the
/// `fast_path_candidates` test) in SQL_REGEX order and decides them by hand; anything unusual (a non-ASCII
/// character where it could matter, `$` or `#` after a word, a word that may start a multi-word keyword) is
/// left to the rules.
fn fast_token(bytes: &[u8], pos: usize) -> Fast {
    let b = bytes[pos];
    let next = bytes.get(pos + 1).copied();
    let token = Fast::Token;
    match b {
        // rule 3, `\s+?`: one character (newlines are rule 2's)
        b'\t' | 0x0b | 0x0c | 0x1c..=b' ' => token(WHITESPACE, 1),
        // rule 2, `(\r\n|\r|\n)`
        b'\n' => token(NEWLINE, 1),
        b'\r' => token(NEWLINE, if next == Some(b'\n') { 2 } else { 1 }),
        // rule 46, `[;:()\[\],\.]`, the only rule these can start
        b',' | b'(' | b')' | b';' | b']' => token(PUNCTUATION, 1),
        // rule 20 (`\.\d+`) needs a digit after the dot; otherwise rule 46
        b'.' => match next {
            Some(c) if c.is_ascii_digit() || c >= 0x80 => Fast::Rules,
            _ => token(PUNCTUATION, 1),
        },
        // rule 48, `[<>=~!]+`; `<` may also start rule 47's `<@`
        b'=' | b'>' | b'!' | b'~' | b'<' => {
            if b == b'<' && next == Some(b'@') {
                return Fast::Rules;
            }
            let len = bytes[pos..]
                .iter()
                .take_while(|&&c| matches!(c, b'<' | b'>' | b'=' | b'~' | b'!'))
                .count();
            token(COMPARISON, len)
        }
        b'a'..=b'z' | b'A'..=b'Z' | b'_' => fast_word(bytes, pos),
        b'\'' => fast_string(bytes, pos),
        b'0'..=b'9' => fast_number(bytes, pos),
        _ => Fast::Rules,
    }
}

/// Rule 22, `'(''|\\'|[^'])*'`. Taking the first alternative that fits at every step is the path Python's
/// backtracking tries first; when it reaches a lone `'` the match ends there. Only when it runs off the end
/// (an unclosed string) would Python backtrack, and that is left to the rule.
fn fast_string(bytes: &[u8], pos: usize) -> Fast {
    let mut p = pos + 1;
    loop {
        match bytes.get(p) {
            None => return Fast::Rules,
            Some(b'\'') if bytes.get(p + 1) == Some(&b'\'') => p += 2,
            Some(b'\'') => return Fast::Token(STRING, p + 1 - pos),
            Some(b'\\') if bytes.get(p + 1) == Some(&b'\'') => p += 2,
            // any other character, a byte at a time (UTF-8 continuation bytes are neither ' nor \)
            Some(_) => p += 1,
        }
    }
}

/// `[_A-Z]` of rules 20 and 21 under IGNORECASE, for an ASCII character (or none).
fn letter_or_underscore(c: Option<u8>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
}

fn ascii_digits_end(bytes: &[u8], mut i: usize) -> usize {
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    i
}

/// A number starting with an ASCII digit: rules 18-21 in order (rule 45 when they all fail). `\d` is Unicode
/// in Python, so a non-ASCII character next to a digit run leaves the token to the rules.
fn fast_number(bytes: &[u8], pos: usize) -> Fast {
    let at = |i: usize| bytes.get(i).copied();
    let non_ascii = |i: usize| at(i).is_some_and(|c| c >= 0x80);
    // rule 18, `-?0x[\dA-F]+`
    if bytes[pos] == b'0' && matches!(at(pos + 1), Some(b'x' | b'X')) {
        let mut e = pos + 2;
        while at(e).is_some_and(|c| c.is_ascii_hexdigit()) {
            e += 1;
        }
        if non_ascii(e) {
            return Fast::Rules;
        }
        if e > pos + 2 {
            return Fast::Token(HEXADECIMAL, e - pos);
        }
    }
    let d1 = ascii_digits_end(bytes, pos);
    if non_ascii(d1) {
        return Fast::Rules;
    }
    // the digits after a dot right after the first run, if there is a dot
    let fraction = if at(d1) == Some(b'.') {
        let d2 = ascii_digits_end(bytes, d1 + 1);
        if non_ascii(d2) {
            return Fast::Rules;
        }
        Some(d2)
    } else {
        None
    };
    // rule 19, `\d+(\.\d+)?E-?\d+`: with the fraction if it has digits, else without (where a dot stops it)
    let mantissa_end = match fraction {
        Some(d2) if d2 > d1 + 1 => d2,
        _ => d1,
    };
    if matches!(at(mantissa_end), Some(b'e' | b'E')) {
        let mut r = mantissa_end + 1;
        if at(r) == Some(b'-') {
            r += 1;
        }
        let d3 = ascii_digits_end(bytes, r);
        if non_ascii(d3) {
            return Fast::Rules;
        }
        if d3 > r {
            return Fast::Token(FLOAT, d3 - pos);
        }
    }
    // rule 20, `(?![_A-Z])-?(\d+(\.\d*)|\.\d+)(?![_A-Z])`: when a letter follows, `\d*` gives back one digit
    if let Some(d2) = fraction {
        if !letter_or_underscore(at(d2)) {
            return Fast::Token(FLOAT, d2 - pos);
        }
        if d2 > d1 + 1 {
            return Fast::Token(FLOAT, d2 - 1 - pos);
        }
    }
    // rule 21, `(?![_A-Z])-?\d+(?![_A-Z])`: likewise `\d+`
    if !letter_or_underscore(at(d1)) {
        return Fast::Token(INTEGER, d1 - pos);
    }
    if d1 - pos >= 2 {
        return Fast::Token(INTEGER, d1 - 1 - pos);
    }
    // a digit then a letter: rule 45's word
    Fast::Rules
}

/// A word starting with an ASCII letter or `_`: rules 13, 15, 16, 17, 26-44 and 45 in that order.
fn fast_word(bytes: &[u8], pos: usize) -> Fast {
    let mut end = pos + 1;
    while end < bytes.len() && is_ascii_word(bytes[end]) {
        end += 1;
    }
    let after = bytes.get(end).copied();
    // past a non-ASCII character `\w` and `\b` need the Unicode tables; `$` and `#` continue rule 45's word
    if after.is_some_and(|c| c >= 0x80 || c == b'$' || c == b'#') {
        return Fast::Rules;
    }
    let len = end - pos;
    let word = &bytes[pos..end];
    if bytes[pos] != b'_' {
        let mut buf = [0u8; 8];
        let upper = (len <= buf.len()).then(|| {
            let up = &mut buf[..len];
            up.copy_from_slice(word);
            up.make_ascii_uppercase();
            &*up
        });
        // rule 13
        if upper.is_some_and(|u| RULE_13_WORDS.contains(&u)) {
            return Fast::Token(KEYWORD, len);
        }
        // rule 15, `[A-Z]\w*(?=\s*\.(?!\d))`
        let mut j = end;
        while j < bytes.len() && is_py_space(bytes[j]) {
            j += 1;
        }
        match bytes.get(j) {
            Some(&c) if c >= 0x80 => return Fast::Rules,
            Some(b'.') => match bytes.get(j + 1) {
                Some(&c) if c >= 0x80 => return Fast::Rules,
                Some(c) if c.is_ascii_digit() => {}
                _ => return Fast::Token(NAME, len),
            },
            _ => {}
        }
        // rule 16, `(?<=\.)[A-Z]\w*`
        if pos > 0 && bytes[pos - 1] == b'.' {
            return Fast::Token(NAME, len);
        }
        // rule 17, `[A-Z]\w*(?=\()`
        if after == Some(b'(') {
            return Fast::Token(NAME, len);
        }
        // rules 26-44
        if let Some(list) = upper.and_then(phrase_rules) {
            return Fast::Phrase(list, len);
        }
    }
    // rule 45, `\w[$#\w]*`: a keyword or a name
    match std::str::from_utf8(word) {
        Ok(value) => Fast::Token(keyword_type(value), len),
        Err(_) => Fast::Rules,
    }
}

/// Every token type `tokenize` can give.
pub(crate) fn type_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = RULES
        .iter()
        .filter_map(|r| match r.action {
            Action::Token(t) => Some(t),
            Action::Keyword => None,
        })
        .chain(KEYWORDS.iter().map(|&(_, t)| t))
        .chain([
            "Token.Name",
            "Token.Error",
            "Token.Literal",
            "Token.Comment.Multiline",
            "Token.Comment.Multiline.Hint",
        ])
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// `sqlparse.lexer.tokenize(text)`: (token type, text) pairs.
pub(crate) fn tokenize(text: &str) -> Vec<(&'static str, &str)> {
    let spans = find_delimited_spans(text);
    let rules = rules();
    let mut out = Vec::new();
    let mut pos = 0;
    'outer: while pos < text.len() {
        if let Some((end, ttype)) = spans.resolve(pos) {
            out.push((ttype, &text[pos..end]));
            pos = end;
            continue;
        }
        if text.as_bytes()[pos] < 0x80 {
            match fast_token(text.as_bytes(), pos) {
                Fast::Token(ttype, len) => {
                    out.push((ttype, &text[pos..pos + len]));
                    pos += len;
                    continue;
                }
                Fast::Phrase(list, len) => {
                    let rest = &text[pos..];
                    for &i in list {
                        let rule = &rules.all[i];
                        if rule.prefilter.as_ref().is_some_and(|p| !p.is_match(rest)) {
                            continue;
                        }
                        if let Ok(Some(m)) = rule.re.find(rest) {
                            let ttype = match rule.action {
                                Action::Token(t) => t,
                                Action::Keyword => keyword_type(&rest[..m.end()]),
                            };
                            out.push((ttype, &rest[..m.end()]));
                            pos += m.end();
                            continue 'outer;
                        }
                    }
                    out.push((keyword_type(&rest[..len]), &rest[..len]));
                    pos += len;
                    continue;
                }
                Fast::Rules => {}
            }
        }
        let prev = text[..pos].chars().next_back();
        let rest = &text[pos..];
        let Some(here) = rest.chars().next() else {
            break;
        };
        for rule in rules.candidates(here) {
            let allowed = match rule.before {
                Before::Any => true,
                Before::In(r) => prev.is_some_and(|c| in_ranges(r, c)),
                Before::NotIn(r) => prev.is_none_or(|c| !in_ranges(r, c)),
            };
            if !allowed {
                continue;
            }
            if rule.prefilter.as_ref().is_some_and(|p| !p.is_match(rest)) {
                continue;
            }
            let Ok(Some(m)) = rule.re.find(rest) else {
                continue;
            };
            let value = &rest[..m.end()];
            let ttype = match rule.action {
                Action::Token(t) => t,
                Action::Keyword => keyword_type(value),
            };
            out.push((ttype, value));
            pos += m.end();
            continue 'outer;
        }
        let c = here.len_utf8();
        out.push(("Token.Error", &rest[..c]));
        pos += c;
    }
    out
}

#[cfg(test)]
mod fast_path_tests {
    use super::*;

    /// The hash index finds every keyword, and nothing else, as a binary search over the sorted table does.
    #[test]
    fn keyword_index_matches_table() {
        assert!(
            KEYWORDS.windows(2).all(|w| w[0].0 < w[1].0),
            "KEYWORDS sorted, no repeats"
        );
        let search = |w: &str| match KEYWORDS.binary_search_by(|(k, _)| k.cmp(&w)) {
            Ok(i) => KEYWORDS[i].1,
            Err(_) => "Token.Name",
        };
        for &(k, ttype) in KEYWORDS.iter() {
            assert_eq!(keyword_type(k), ttype, "{k}");
            assert_eq!(
                keyword_type(&k.to_lowercase()),
                search(&k.to_uppercase()),
                "{k}"
            );
            for extra in ["X", "_", " "] {
                let w = format!("{k}{extra}");
                assert_eq!(keyword_type(&w), search(&w.to_uppercase()), "{w}");
            }
        }
        for w in ["", "x", "selec", "ſelect", "naïve", "ORDER  BY"] {
            assert_eq!(
                keyword_type(w),
                search(&super::super::pystr::upper(w)),
                "{w}"
            );
        }
    }

    /// The rules each fast-path character can start, as `fast_token` and `fast_word` assume them.
    #[test]
    fn fast_path_candidates() {
        let r = rules();
        let cands = |b: u8| r.ascii[b as usize].clone();
        for b in [b'\t', 0x0b, 0x0c, 0x1c, 0x1d, 0x1e, 0x1f, b' '] {
            assert_eq!(cands(b), vec![3], "{b:#x}");
        }
        for b in *b"\n\r" {
            assert_eq!(cands(b), vec![2, 3], "{b:#x}");
        }
        for b in *b",();]" {
            assert_eq!(cands(b), vec![46], "{}", b as char);
        }
        assert_eq!(cands(b'.'), vec![20, 46]);
        for b in *b"=>!~" {
            assert_eq!(cands(b), vec![48], "{}", b as char);
        }
        assert_eq!(cands(b'<'), vec![47, 48]);
        assert_eq!(cands(b'_'), vec![45]);
        assert_eq!(cands(b'\''), vec![22]);
        assert_eq!(cands(b'0'), vec![18, 19, 20, 21, 45]);
        for b in b'1'..=b'9' {
            assert_eq!(cands(b), vec![19, 20, 21, 45], "{}", b as char);
        }
        for (i, rule) in r.all.iter().enumerate().take(45).skip(26) {
            assert!(
                matches!(rule.before, Before::Any),
                "rule {i} has a lookbehind"
            );
        }
        for b in (b'a'..=b'z').chain(b'A'..=b'Z') {
            let c = cands(b);
            assert!(c.contains(&45), "{}", b as char);
            for i in c {
                assert!(
                    matches!(i, 13 | 15 | 16 | 17 | 26..=45),
                    "{} can start rule {i}",
                    b as char
                );
            }
        }
    }

    /// Rules 26-44 match only from the words `phrase_rules` gives them: probed with every keyword of sqlparse's
    /// lists (and those words with a letter added) followed by each kind of continuation.
    #[test]
    fn phrase_rules_by_start_word() {
        let r = rules();
        let mut words: Vec<String> = KEYWORDS.iter().map(|(k, _)| k.to_string()).collect();
        words.extend(KEYWORDS.iter().map(|(k, _)| format!("{k}X")));
        words.extend(
            [
                "BY",
                "TIME",
                "ZONE",
                "ALL",
                "KEY",
                "FOR",
                "VIEW",
                "PRECISION",
                "WITH'",
            ]
            .map(String::from),
        );
        let tails = [
            " ",
            "",
            "'",
            " JOIN",
            " OUTER JOIN",
            " BY",
            " NULL",
            " EXISTS",
            " NOT EXISTS",
            " TIME ZONE 'x'",
            "' TIME ZONE 'x'",
            " 5",
            " NULLS FIRST",
            " ALL",
            " OR REPLACE",
            " PRECISION",
            " KEY",
            " FOR",
            " LIKE",
            " REGEXP BINARY",
            " LOOP",
            " VIEW EXPLODE",
            " IF",
            " CASE",
        ];
        for w in &words {
            let word: String = w
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if word.is_empty() {
                continue;
            }
            let allowed = phrase_rules(word.to_ascii_uppercase().as_bytes()).unwrap_or(&[]);
            for tail in tails {
                let probe = format!("{w}{tail}");
                for (i, rule) in r.all.iter().enumerate().take(45).skip(26) {
                    let hit = rule.re.find(&probe).unwrap().is_some();
                    assert!(!hit || allowed.contains(&i), "rule {i} matched {probe:?}");
                }
            }
        }
    }

    /// The whole tokenizer with the fast path against the rules alone.
    #[test]
    fn fast_path_matches_rules() {
        let texts = [
            "select a.b, f(x), t . c, x.1, y .5, \"q\" from t where a<=b and c<@d and e => f;\r\n",
            "SELECT case when x in (1) then 'a' end AS z FROM t left join u using (id) group by 1 order by 2",
            "a$b a#b a_b _x __ x\u{a0}.y x\t\n. y CREATE OR REPLACE with'x at time zone 'UTC' go 5 not null",
            "x .\u{663} y\r z\x1c.w Values(1) inner  join from. t.from as.x desc nulls first ilike ORDER\tBY",
            "1 12 0x1F 0x 0xg 1.5 1. 1.e 1.5e3 1.5e-3 1e 1e- 1.5ex 12abc 1abc 1_ 12_ 1.5x 1.55x 1.x 3.14.15 7\u{663}",
            "'a' '' 'it''s' 'a\\'b' 'a\\' x 'x\u{e9}y' '''' 'unclosed 'a'' 'b\\'",
            "not like NOT  REGEXP binary not null if not exists end loop union all go 5 with' time zone 'z' at x",
            "LEFT OUTER JOIN cross natural x left x null nulls asc desc create or replace double precision 0x1g",
        ];
        let rules = rules();
        for t in texts {
            let fast = tokenize(t);
            let mut slow = Vec::new();
            let mut pos = 0;
            'outer: while pos < t.len() {
                let prev = t[..pos].chars().next_back();
                let rest = &t[pos..];
                let here = rest.chars().next().unwrap();
                for rule in rules.candidates(here) {
                    let allowed = match rule.before {
                        Before::Any => true,
                        Before::In(r) => prev.is_some_and(|c| in_ranges(r, c)),
                        Before::NotIn(r) => prev.is_none_or(|c| !in_ranges(r, c)),
                    };
                    if !allowed {
                        continue;
                    }
                    if let Ok(Some(m)) = rule.re.find(rest) {
                        let ttype = match rule.action {
                            Action::Token(t) => t,
                            Action::Keyword => keyword_type(&rest[..m.end()]),
                        };
                        slow.push((ttype, &rest[..m.end()]));
                        pos += m.end();
                        continue 'outer;
                    }
                }
                slow.push(("Token.Error", &rest[..here.len_utf8()]));
                pos += here.len_utf8();
            }
            assert_eq!(fast, slow, "{t:?}");
        }
    }
}

#[cfg(test)]
mod slow_path_census {
    use super::*;

    #[test]
    #[ignore]
    fn census() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/corpus");
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut total = 0;
        for e in std::fs::read_dir(dir).unwrap() {
            let t = std::fs::read_to_string(e.unwrap().path()).unwrap();
            let mut pos = 0;
            for (ttype, v) in tokenize(&t) {
                total += 1;
                let b = t.as_bytes()[pos];
                if b >= 0x80 || !matches!(fast_token(t.as_bytes(), pos), Fast::Token(..)) {
                    let k = format!("{ttype} {:?}", v.chars().next().unwrap());
                    let key = if ttype.contains("Keyword")
                        || ttype.contains("String")
                        || ttype.contains("Number")
                    {
                        ttype.to_string()
                    } else {
                        k
                    };
                    *counts.entry(key).or_default() += 1;
                }
                pos += v.len();
            }
        }
        let mut v: Vec<_> = counts.into_iter().collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.1));
        let slow: usize = v.iter().map(|x| x.1).sum();
        println!("total {total}, slow {slow}");
        for (k, n) in v.iter().take(25) {
            println!("{n:8} {k}");
        }
    }
}
