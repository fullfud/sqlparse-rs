//! Port of `sqlparse.split` (sqlparse 0.6.1): `engine/statement_splitter.py`, the strip-semicolon filter and
//! `str(stmt).strip()`.
//!
//! Token types are compared as sqlparse compares them: `ttype is T.X` is equality, `ttype in T.X` is "X or a
//! subtype of X", and `ttype in (T.A, T.B)` (a tuple of types) is equality with one of them. A statement is a
//! run of consecutive tokens, so its text is the slice of the input they cover.

use super::{lexer, pystr};

const PUNCTUATION: &str = "Token.Punctuation";
const KEYWORD: &str = "Token.Keyword";
const KEYWORD_DDL: &str = "Token.Keyword.DDL";
const WHITESPACE: &str = "Token.Text.Whitespace";
const NEWLINE: &str = "Token.Text.Whitespace.Newline";
const COMMENT_SINGLE: &str = "Token.Comment.Single";
const COMMENT_MULTILINE: &str = "Token.Comment.Multiline";

/// `ttype in parent`: the type itself or one of its subtypes.
pub(crate) fn is_a(ttype: &str, parent: &str) -> bool {
    ttype.len() >= parent.len()
        && ttype.starts_with(parent)
        && (ttype.len() == parent.len() || ttype.as_bytes()[parent.len()] == b'.')
}

/// `value.upper() == word` for an upper-case ASCII `word`, without allocating for ASCII values.
pub(crate) fn upper_is(value: &str, word: &str) -> bool {
    if value.is_ascii() {
        value.eq_ignore_ascii_case(word)
    } else {
        pystr::upper(value) == word
    }
}

/// `value.split()[0]` (None where Python raises IndexError: no word at all).
pub(crate) fn first_word(value: &str) -> Option<&str> {
    value.split(pystr::isspace).find(|w| !w.is_empty())
}

#[derive(Default)]
struct Splitter {
    block_stack: Vec<&'static str>,
    parenthesis_level: i64,
    unconfirmed_start: Option<&'static str>,
    is_create: bool,
    seen_begin: bool,
    consume_ws: bool,
    level: i64,
}

impl Splitter {
    fn top_is(&self, what: &str) -> bool {
        self.block_stack.last().is_some_and(|t| *t == what)
    }

    fn handle_nested_block(&mut self, unified: &str) -> Option<i64> {
        match unified {
            "FOR" => {
                self.unconfirmed_start = Some("FOR");
                return Some(0);
            }
            "WHILE" => {
                self.unconfirmed_start = Some("WHILE");
                return Some(0);
            }
            _ => {}
        }
        if unified == "LOOP" || unified == "DO" {
            if let Some(start) = self
                .unconfirmed_start
                .filter(|s| *s == "FOR" || *s == "WHILE")
            {
                self.block_stack.push(start);
                self.unconfirmed_start = None;
                return Some(1);
            }
            if unified == "LOOP" {
                self.block_stack.push("LOOP");
                return Some(1);
            }
        }
        match unified {
            "IF" => {
                self.block_stack.push("IF");
                Some(1)
            }
            "CASE" => {
                self.block_stack.push("CASE");
                Some(1)
            }
            _ => None,
        }
    }

    fn handle_closing_keyword(&mut self, unified: &str) -> i64 {
        let closes = match unified {
            "END IF" => self.top_is("IF"),
            "END FOR" => self.top_is("FOR"),
            "END WHILE" => self.top_is("WHILE"),
            "END LOOP" => self
                .block_stack
                .last()
                .is_some_and(|t| matches!(*t, "LOOP" | "FOR" | "WHILE")),
            "END CASE" => self.top_is("CASE"),
            "END" => {
                self.block_stack.pop();
                return -1;
            }
            _ => false,
        };
        if closes {
            self.block_stack.pop();
            -1
        } else {
            0
        }
    }

    fn change_splitlevel(&mut self, ttype: &str, value: &str) -> i64 {
        if ttype == PUNCTUATION && value == ";" {
            self.unconfirmed_start = None;
            if self.seen_begin {
                self.seen_begin = false;
                if self.top_is("BEGIN") {
                    self.block_stack.pop();
                    return -1;
                }
            }
            return 0;
        }
        if ttype == PUNCTUATION && value == "(" {
            self.parenthesis_level += 1;
            return 1;
        } else if ttype == PUNCTUATION && value == ")" {
            self.parenthesis_level = (self.parenthesis_level - 1).max(0);
            return -1;
        } else if !is_a(ttype, KEYWORD) {
            return 0;
        }
        // `value.upper()`, on the stack for the usual short ASCII keyword
        let mut buf = [0u8; 32];
        let owned;
        let unified: &str = if value.is_ascii() && value.len() <= buf.len() {
            let up = &mut buf[..value.len()];
            up.copy_from_slice(value.as_bytes());
            up.make_ascii_uppercase();
            std::str::from_utf8(up).unwrap_or_default()
        } else {
            owned = pystr::upper(value);
            &owned
        };
        if ttype == KEYWORD_DDL && unified.starts_with("CREATE") {
            self.is_create = true;
            return 0;
        }
        if unified == "DECLARE" && self.is_create && self.block_stack.is_empty() {
            self.block_stack.push("DECLARE");
            return 1;
        }
        if unified == "BEGIN" {
            self.seen_begin = true;
            if self.top_is("DECLARE") {
                self.block_stack.pop();
                self.block_stack.push("BEGIN");
                return 0;
            }
            self.block_stack.push("BEGIN");
            return 1;
        }
        // sqlparse also accepts `ttype is T.Name` here, but a Name never gets past the Keyword check above.
        if self.seen_begin
            && ttype == KEYWORD
            && matches!(
                unified,
                "TRANSACTION"
                    | "WORK"
                    | "TRAN"
                    | "DISTRIBUTED"
                    | "DEFERRED"
                    | "IMMEDIATE"
                    | "EXCLUSIVE"
            )
        {
            self.seen_begin = false;
            if self.top_is("BEGIN") {
                self.block_stack.pop();
                return -1;
            }
            return 0;
        }
        if self.block_stack.contains(&"BEGIN")
            && let Some(res) = self.handle_nested_block(unified)
        {
            return res;
        }
        self.handle_closing_keyword(unified)
    }
}

/// `StatementSplitter.process`, fed one token at a time.
#[derive(Default)]
pub(crate) struct Feed(Splitter);

impl Feed {
    /// Takes the next token; true when a statement ended before it (the token then starts the next one).
    pub(crate) fn token(&mut self, ttype: &'static str, value: &str) -> bool {
        let mut ended = false;
        if self.0.consume_ws && ttype != WHITESPACE && ttype != COMMENT_SINGLE {
            self.0 = Splitter::default();
            ended = true;
        }
        let s = &mut self.0;
        s.level += s.change_splitlevel(ttype, value);
        if ttype == PUNCTUATION && value == ";" {
            s.seen_begin = false;
            if s.level <= 0 && !s.block_stack.contains(&"BEGIN") {
                s.consume_ws = true;
            }
        } else if ttype == KEYWORD && first_word(value) == Some("GO") {
            s.consume_ws = true;
        } else if !matches!(
            ttype,
            WHITESPACE | NEWLINE | COMMENT_SINGLE | COMMENT_MULTILINE
        ) && !(ttype == KEYWORD && upper_is(value, "BEGIN"))
        {
            s.seen_begin = false;
        }
        ended
    }
}

/// Whether the tokens left at the end of the text make a statement: sqlparse's splitter drops them when they
/// are all whitespace.
pub(crate) fn is_statement<'a>(mut types: impl Iterator<Item = &'a str>) -> bool {
    let mut any = false;
    let all_ws = types.all(|t| {
        any = true;
        is_a(t, WHITESPACE)
    });
    any && !all_ws
}

/// `StatementSplitter().process(tokenize(text))`: each statement as the range of tokens it holds.
pub(crate) fn statements(tokens: &[(&'static str, &str)]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut feed = Feed::default();
    let mut start = 0;
    for (i, &(ttype, value)) in tokens.iter().enumerate() {
        if feed.token(ttype, value) {
            out.push(start..i);
            start = i;
        }
    }
    if is_statement(tokens[start..].iter().map(|&(t, _)| t)) {
        out.push(start..tokens.len());
    }
    out
}

/// `sqlparse.split(text, strip_semicolon=...)`.
pub(crate) fn split(text: &str, strip_semicolon: bool) -> Vec<String> {
    let tokens = lexer::tokenize(text);
    // every token is a slice of `text` and a statement is a run of consecutive tokens: its text is the slice
    // from its first token's start to its last token's end
    let offset = |v: &str| v.as_ptr() as usize - text.as_ptr() as usize;
    statements(&tokens)
        .into_iter()
        .map(|range| {
            let mut stmt = &tokens[range];
            if strip_semicolon {
                while let Some((ttype, value)) = stmt.last() {
                    if is_a(ttype, WHITESPACE) || *value == ";" {
                        stmt = &stmt[..stmt.len() - 1];
                    } else {
                        break;
                    }
                }
            }
            match (stmt.first(), stmt.last()) {
                (Some((_, first)), Some((_, last))) => {
                    pystr::strip(&text[offset(first)..offset(last) + last.len()]).to_string()
                }
                _ => String::new(),
            }
        })
        .collect()
}
