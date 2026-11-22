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

/// `Lexer.is_keyword`: the keyword's token type, or `Token.Name`.
fn keyword_type(value: &str) -> &'static str {
    let up = value.to_uppercase();
    match KEYWORDS.binary_search_by(|(k, _)| k.as_bytes().cmp(up.as_bytes())) {
        Ok(i) => KEYWORDS[i].1,
        Err(_) => "Token.Name",
    }
}

/// `keywords.DelimitedSpans`: openers by start, closers by tag.
struct Spans<'a> {
    openers: HashMap<usize, (usize, &'a str, &'static str)>,
    closers: HashMap<&'a str, (Vec<usize>, Vec<usize>)>,
}

impl Spans<'_> {
    fn resolve(&self, pos: usize) -> Option<(usize, &'static str)> {
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
