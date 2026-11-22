//! Port of the statement filters of `sqlparse.filters` (sqlparse 0.6.1) that work on the grouped tree:
//! `StripCommentsFilter`, `StripWhitespaceFilter`, `SpacesAroundOperatorsFilter` (others.py),
//! `ReindentFilter` (reindent.py) and `AlignedIndentFilter` (aligned_indent.py).

use super::grouping::{RECURSION_ERROR, Stack};
use super::pyunicode_tables::WORD;
use super::split::first_word;
use super::tree::{Cls, Id, ROOT, Tree, tt};
use super::{lexer::in_ranges, pystr};

fn index_error() -> String {
    "IndexError: list index out of range".into()
}

fn stop_iteration() -> String {
    // StopIteration inside the generator FilterStack.run (PEP 479)
    "RuntimeError: generator raised StopIteration".into()
}

/// The last character of `str(token)`.
fn last_char(t: &Tree, id: Id) -> Option<char> {
    if !t.is_group(id) {
        return t.value(id).chars().next_back();
    }
    t.flatten(id)
        .into_iter()
        .rev()
        .find_map(|leaf| t.value(leaf).chars().next_back())
}

// ---------------------------------------------------------------- StripCommentsFilter

/// `_get_insert_token`: the line breaks at the end of the comment (`([\r\n]+) *$`), or a space.
fn insert_token(t: &mut Tree, comment: Id) -> Id {
    match trailing_breaks(t.value(comment)) {
        Some(breaks) => {
            let breaks = breaks.to_string();
            t.new_token(tt::NEWLINE, breaks)
        }
        None => t.new_token(tt::WHITESPACE, " ".into()),
    }
}

/// Group 1 of `re.search(r'([\r\n]+) *$', value)`: the leftmost run of line breaks followed only by spaces
/// up to the end (or up to a final newline, which `$` also accepts).
fn trailing_breaks(value: &str) -> Option<&str> {
    let b = value.as_bytes();
    let n = b.len();
    let is_break = |c: u8| c == b'\r' || c == b'\n';
    let at_end = |m: usize| m == n || (m + 1 == n && b[m] == b'\n');
    for p in 0..n {
        if !is_break(b[p]) {
            continue;
        }
        let mut run_end = p;
        while run_end < n && is_break(b[run_end]) {
            run_end += 1;
        }
        for k in (p + 1..=run_end).rev() {
            let mut sp = k;
            while sp < n && b[sp] == b' ' {
                sp += 1;
            }
            if (k..=sp).rev().any(at_end) {
                return Some(&value[p..k]);
            }
        }
    }
    None
}

fn strip_comments_list(t: &mut Tree, l: Id) {
    let is_comment = |t: &Tree, id: Id| t.cls(id) == Cls::Comment || t.ttype_is_a(id, tt::COMMENT);
    let hints = [tt::MULTILINE_HINT, tt::SINGLE_HINT];
    let mut cur = t.next_by(l, -1, is_comment);
    while let Some((tidx, token)) = cur {
        let is_hint = if t.ttype_in(token, &hints) {
            true
        } else if t.cls(token) == Cls::Comment {
            t.tokens(token)
                .first()
                .is_some_and(|&c| t.ttype_in(c, &hints))
        } else {
            false
        };
        if is_hint {
            cur = t.next_by(l, tidx as isize, is_comment);
            continue;
        }
        let prev = t.token_prev(l, Some(tidx), false, false).map(|p| p.1);
        let next = t.token_next(l, Some(tidx), false, false).map(|n| n.1);
        let mut tidx = tidx as isize;
        let replace_in_place = match (prev, next) {
            (Some(p), Some(n)) => {
                !(t.n[p].is_whitespace
                    || t.matches(p, tt::PUNCTUATION, &["("])
                    || t.n[n].is_whitespace
                    || t.matches(n, tt::PUNCTUATION, &[")"]))
            }
            _ => false,
        };
        if replace_in_place {
            let ins = insert_token(t, token);
            t.n[l].tokens[tidx as usize] = ins;
        } else {
            if let Some(p) = prev
                && !t.matches(p, tt::PUNCTUATION, &["("])
            {
                let ins = insert_token(t, token);
                t.n[l].tokens.insert(tidx as usize, ins);
            }
            if let Some(pos) = t.n[l].tokens.iter().position(|&x| x == token) {
                t.n[l].tokens.remove(pos);
            }
            tidx -= 1;
        }
        cur = t.next_by(l, tidx, is_comment);
    }
}

/// `StripCommentsFilter.process`.
pub(crate) fn strip_comments(t: &mut Tree, l: Id) {
    let mut i = 0;
    while let Some(&s) = t.tokens(l).get(i) {
        if t.is_group(s) {
            strip_comments(t, s);
        }
        i += 1;
    }
    strip_comments_list(t, l);
}

// ---------------------------------------------------------------- StripWhitespaceFilter

fn stripws_default(t: &mut Tree, l: Id) {
    let mut last_was_ws = false;
    let mut is_first_char = true;
    let mut i = 0;
    while let Some(&token) = t.tokens(l).get(i) {
        let ws = t.n[token].is_whitespace;
        if ws {
            let v = if last_was_ws || is_first_char {
                ""
            } else {
                " "
            };
            t.set_value(token, v.into());
        }
        last_was_ws = ws;
        is_first_char = false;
        i += 1;
    }
}

fn stripws_identifierlist(t: &mut Tree, l: Id) {
    let mut last_nl: Option<Id> = None;
    for token in t.tokens(l).to_vec() {
        if let Some(nl) = last_nl
            && t.ttype_is(token, tt::PUNCTUATION)
            && t.value(token) == ","
            && let Some(pos) = t.n[l].tokens.iter().position(|&x| x == nl)
        {
            t.n[l].tokens.remove(pos);
        }
        last_nl = t.n[token].is_whitespace.then_some(token);
    }
    stripws_default(t, l);
}

fn stripws_parenthesis(t: &mut Tree, l: Id) -> Result<(), String> {
    let toks = t.tokens(l);
    if !(toks.len() >= 2
        && t.matches(toks[0], tt::PUNCTUATION, &["("])
        && t.matches(toks[toks.len() - 1], tt::PUNCTUATION, &[")"]))
    {
        stripws_default(t, l);
        return Ok(());
    }
    while t.n[t.tokens(l)[1]].is_whitespace {
        t.n[l].tokens.remove(1);
    }
    loop {
        let len = t.tokens(l).len();
        if !t.n[t.tokens(l)[len - 2]].is_whitespace {
            break;
        }
        t.n[l].tokens.remove(len - 2);
    }
    let len = t.tokens(l).len();
    let inner = t.tokens(l)[len - 2];
    if t.is_group(inner) {
        loop {
            let last = *t.tokens(inner).last().ok_or_else(index_error)?;
            if !t.n[last].is_whitespace {
                break;
            }
            t.n[inner].tokens.pop();
        }
    }
    stripws_default(t, l);
    Ok(())
}

fn strip_whitespace_at(t: &mut Tree, l: Id, depth: usize) -> Result<(), String> {
    let mut i = 0;
    while let Some(&s) = t.tokens(l).get(i) {
        if t.is_group(s) {
            strip_whitespace_at(t, s, depth + 1)?;
        }
        i += 1;
    }
    match t.cls(l) {
        Cls::IdentifierList => stripws_identifierlist(t, l),
        Cls::Parenthesis => stripws_parenthesis(t, l)?,
        _ => stripws_default(t, l),
    }
    if depth == 0
        && let Some(&last) = t.tokens(l).last()
        && t.n[last].is_whitespace
    {
        t.n[l].tokens.pop();
    }
    Ok(())
}

/// `StripWhitespaceFilter.process`.
pub(crate) fn strip_whitespace(t: &mut Tree) -> Result<(), String> {
    strip_whitespace_at(t, ROOT, 0)
}

// ---------------------------------------------------------------- SpacesAroundOperatorsFilter

fn spaces_around_operators_list(t: &mut Tree, l: Id) {
    let is_op = |t: &Tree, id: Id| t.ttype_in(id, &[tt::OPERATOR, tt::COMPARISON]);
    let mut cur = t.next_by(l, -1, is_op);
    while let Some((mut tidx, _)) = cur {
        if let Some((_, next)) = t.token_next(l, Some(tidx), false, false)
            && !t.ttype_is(next, tt::WHITESPACE)
        {
            let sp = t.new_token(tt::WHITESPACE, " ".into());
            t.insert_after(l, tidx, sp, true);
        }
        if let Some((_, prev)) = t.token_prev(l, Some(tidx), false, false)
            && !t.ttype_is(prev, tt::WHITESPACE)
        {
            let sp = t.new_token(tt::WHITESPACE, " ".into());
            t.insert_before(l, tidx, sp);
            tidx += 1;
        }
        cur = t.next_by(l, tidx as isize, is_op);
    }
}

/// `SpacesAroundOperatorsFilter.process`.
pub(crate) fn spaces_around_operators(t: &mut Tree, l: Id) {
    let mut i = 0;
    while let Some(&s) = t.tokens(l).get(i) {
        if t.is_group(s) {
            spaces_around_operators(t, s);
        }
        i += 1;
    }
    spaces_around_operators_list(t, l);
}

// ---------------------------------------------------------------- split words (Token.match(..., regex=True))

/// The patterns the indent filters search keywords with.
#[derive(Clone, Copy)]
enum Pat {
    /// A literal anywhere.
    Lit(&'static str),
    /// `literal$`.
    End(&'static str),
    /// AlignedIndentFilter.join_words: `(...)?JOIN\b`, found wherever `JOIN\b` is.
    Join,
    /// AlignedIndentFilter.by_words: `(GROUP|ORDER)\s+BY\b`.
    By,
}

const REINDENT_SPLIT: [Pat; 14] = [
    Pat::Lit("FROM"),
    Pat::End("STRAIGHT_JOIN"),
    Pat::End("JOIN"),
    Pat::Lit("AND"),
    Pat::Lit("OR"),
    Pat::Lit("GROUP BY"),
    Pat::Lit("ORDER BY"),
    Pat::Lit("UNION"),
    Pat::Lit("VALUES"),
    Pat::Lit("SET"),
    Pat::Lit("BETWEEN"),
    Pat::Lit("EXCEPT"),
    Pat::Lit("HAVING"),
    Pat::Lit("LIMIT"),
];

const ALIGNED_SPLIT: [Pat; 14] = [
    Pat::Lit("FROM"),
    Pat::Join,
    Pat::Lit("ON"),
    Pat::By,
    Pat::Lit("WHERE"),
    Pat::Lit("AND"),
    Pat::Lit("OR"),
    Pat::Lit("HAVING"),
    Pat::Lit("LIMIT"),
    Pat::Lit("UNION"),
    Pat::Lit("VALUES"),
    Pat::Lit("SET"),
    Pat::Lit("BETWEEN"),
    Pat::Lit("EXCEPT"),
];

/// One pattern character against one text character, as `re.IGNORECASE` compares them for a str pattern:
/// simple lower-case mapping plus sre's extra pairs (so 'İ', 'ı' match 'i', 'K' (Kelvin) 'k', 'ſ' 's').
fn char_eq(p: u8, c: char, ignorecase: bool) -> bool {
    if !ignorecase {
        return c == p as char;
    }
    if c.is_ascii() {
        return (c as u8).eq_ignore_ascii_case(&p);
    }
    matches!(
        (p.to_ascii_lowercase(), c),
        (b'i', '\u{130}' | '\u{131}') | (b'k', '\u{212a}') | (b's', '\u{17f}')
    )
}

fn lit_at(chars: &[char], at: usize, lit: &str, ic: bool) -> bool {
    at + lit.len() <= chars.len()
        && lit
            .bytes()
            .enumerate()
            .all(|(j, p)| char_eq(p, chars[at + j], ic))
}

fn is_word(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphanumeric() || c == '_'
    } else {
        in_ranges(WORD, c)
    }
}

/// `\b` after a word character: the end, or a character that is not one.
fn word_end(chars: &[char], at: usize) -> bool {
    chars.get(at).is_none_or(|&c| !is_word(c))
}

fn search(p: Pat, s: &str, ic: bool) -> bool {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    match p {
        Pat::Lit(lit) => (0..n).any(|i| lit_at(&chars, i, lit, ic)),
        Pat::End(lit) => (0..n).any(|i| {
            let e = i + lit.len();
            lit_at(&chars, i, lit, ic) && (e == n || (e + 1 == n && chars[e] == '\n'))
        }),
        Pat::Join => (0..n).any(|i| lit_at(&chars, i, "JOIN", ic) && word_end(&chars, i + 4)),
        Pat::By => (0..n).any(|i| {
            if !(lit_at(&chars, i, "GROUP", ic) || lit_at(&chars, i, "ORDER", ic)) {
                return false;
            }
            let mut k = i + 5;
            while k < n && pystr::isspace(chars[k]) {
                k += 1;
            }
            k > i + 5 && lit_at(&chars, k, "BY", ic) && word_end(&chars, k + 2)
        }),
    }
}

/// `token.match(T.Keyword, patterns, regex=True)`.
fn keyword_regex(t: &Tree, id: Id, pats: &[Pat]) -> bool {
    t.ttype_is(id, tt::KEYWORD) && {
        let ic = t.n[id].is_keyword;
        let norm = t.normalized(id);
        pats.iter().any(|&p| search(p, norm, ic))
    }
}

/// Python frames a `token_next_by(m=(T.Keyword, patterns, True))` search reaches above the caller's frame:
/// token_next_by, _token_matching, the lambda, imt, Token.match, and for a token typed exactly Keyword the
/// regex path (`any()`'s generator, the pattern generator, re.compile, re._compile, the RegexFlag value
/// lookup; measured with tools/check_recursion.py).
const REGEX_SEARCH_FRAMES: usize = 5 + 6;

fn search_frames(t: &Tree, l: Id, idx: isize, found: Option<usize>) -> usize {
    let toks = t.tokens(l);
    let from = (idx + 1).max(0) as usize;
    let to = found.map_or(toks.len(), |f| f + 1);
    let scanned = toks.get(from..to.min(toks.len())).unwrap_or(&[]);
    if scanned.is_empty() {
        2
    } else if scanned.iter().any(|&id| t.ttype_is(id, tt::KEYWORD)) {
        REGEX_SEARCH_FRAMES
    } else {
        5
    }
}

/// `_next_token` of both indent filters. `frames` counts the Python frames up to this call's.
fn next_split(
    t: &Tree,
    l: Id,
    idx: isize,
    pats: &[Pat],
    stack: &Stack,
    frames: usize,
) -> Result<Option<(usize, Id)>, String> {
    let found = t.next_by(l, idx, |t, id| keyword_regex(t, id, pats));
    if stack.overflows(frames + REGEX_SEARCH_FRAMES)
        && stack.overflows(frames + search_frames(t, l, idx, found.map(|f| f.0)))
    {
        return Err(RECURSION_ERROR.into());
    }
    if let Some((tidx, token)) = found
        && t.normalized(token) == "BETWEEN"
    {
        let after = next_split(t, l, tidx as isize, pats, stack, frames + 1)?;
        if let Some((tidx, token)) = after
            && t.normalized(token) == "AND"
        {
            return next_split(t, l, tidx as isize, pats, stack, frames + 1);
        }
        return Ok(after);
    }
    Ok(found)
}

/// `Case.get_cases(skip_ws)`.
#[allow(clippy::type_complexity)]
fn get_cases(t: &Tree, l: Id, skip_ws: bool) -> Vec<(Option<Vec<Id>>, Vec<Id>)> {
    #[derive(PartialEq)]
    enum Mode {
        Condition,
        Value,
        Done,
    }
    let mut ret: Vec<(Option<Vec<Id>>, Vec<Id>)> = Vec::new();
    let mut mode = Mode::Condition;
    for &token in t.tokens(l) {
        if t.matches(token, tt::KEYWORD, &["CASE"])
            || (skip_ws && t.ttype_is_a(token, tt::WHITESPACE))
        {
            continue;
        } else if t.matches(token, tt::KEYWORD, &["WHEN"]) {
            ret.push((Some(Vec::new()), Vec::new()));
            mode = Mode::Condition;
        } else if t.matches(token, tt::KEYWORD, &["THEN"]) {
            mode = Mode::Value;
        } else if t.matches(token, tt::KEYWORD, &["ELSE"]) {
            ret.push((None, Vec::new()));
            mode = Mode::Value;
        } else if t.matches(token, tt::KEYWORD, &["END"]) {
            mode = Mode::Done;
        }
        if mode != Mode::Done && ret.is_empty() {
            ret.push((Some(Vec::new()), Vec::new()));
        }
        match mode {
            Mode::Condition => {
                if let Some(cond) = &mut ret.last_mut().expect("pushed above").0 {
                    cond.push(token);
                }
            }
            Mode::Value => ret.last_mut().expect("pushed above").1.push(token),
            Mode::Done => {}
        }
    }
    ret
}

fn str_len(t: &Tree, id: Id) -> i128 {
    pystr::len(&t.to_str(id)) as i128
}

/// `char * n` for the indent character (a negative count gives '').
fn repeat(ch: char, n: i128) -> Result<String, String> {
    if n <= 0 {
        return Ok(String::new());
    }
    if n > 1 << 40 {
        return Err("MemoryError: ".into());
    }
    Ok(std::iter::repeat_n(ch, n as usize).collect())
}

// ---------------------------------------------------------------- ReindentFilter

pub(crate) struct Reindent<'s> {
    ch: char,
    width: i128,
    indent: i128,
    offset: i128,
    wrap_after: i128,
    comma_first: bool,
    indent_columns: bool,
    compact: bool,
    /// `len(self._last_func.value)` (the token keeps that value for good once set).
    last_func_len: Option<i128>,
    /// Whether `str(self._last_stmt)` ends in '\n', once there is a last statement.
    last_stmt_nl: Option<bool>,
    stack: &'s Stack,
}

pub(crate) struct ReindentOptions {
    pub(crate) tabs: bool,
    pub(crate) width: i64,
    pub(crate) indent_after_first: bool,
    pub(crate) indent_columns: bool,
    pub(crate) wrap_after: i64,
    pub(crate) comma_first: bool,
    pub(crate) compact: bool,
}

impl<'s> Reindent<'s> {
    pub(crate) fn new(o: &ReindentOptions, stack: &'s Stack) -> Reindent<'s> {
        Reindent {
            ch: if o.tabs { '\t' } else { ' ' },
            width: o.width.into(),
            indent: i128::from(o.indent_after_first),
            offset: 0,
            wrap_after: o.wrap_after.into(),
            comma_first: o.comma_first,
            indent_columns: o.indent_columns,
            compact: o.compact,
            last_func_len: None,
            last_stmt_nl: None,
            stack,
        }
    }

    fn leading_ws(&self) -> i128 {
        self.offset + self.indent * self.width
    }

    /// `_current_line_len`: walks the tokens before `token`, last one first, up to the line break.
    fn current_line_len(&self, t: &Tree, token: Id) -> Result<i128, String> {
        let mut length: i128 = 0;
        let mut node = token;
        while node != ROOT
            && let Some(parent) = t.n[node].parent
        {
            let idx = t.token_index(parent, node)?;
            let mut stack: Vec<Id> = t.tokens(parent)[..idx].to_vec();
            while let Some(prev) = stack.pop() {
                if t.is_group(prev) {
                    stack.extend_from_slice(t.tokens(prev));
                    continue;
                }
                let value = t.value(prev);
                let size = pystr::len(value) as i128;
                if size == 0 {
                    continue;
                }
                let lines = pystr::splitlines(value);
                if lines.len() == 1 && pystr::len(lines[0]) as i128 == size {
                    length += size;
                    continue;
                }
                let dotted = format!("{value}.");
                let lines = pystr::splitlines(&dotted);
                let tail = pystr::len(lines[lines.len() - 1]) as i128 - 1 + length;
                if tail != 0 {
                    return Ok(tail);
                }
                if lines.len() > 2 {
                    return Ok(pystr::len(lines[lines.len() - 2]) as i128);
                }
                length = pystr::len(lines[0]) as i128;
            }
            node = parent;
        }
        Ok(length)
    }

    fn get_offset(&self, t: &Tree, token: Id) -> Result<i128, String> {
        Ok(self.current_line_len(t, token)? - self.leading_ws().max(0))
    }

    fn nl(&self, t: &mut Tree, offset: i128) -> Result<Id, String> {
        let v = format!("\n{}", repeat(self.ch, self.leading_ws() + offset)?);
        Ok(t.new_token(tt::WHITESPACE, v))
    }

    fn split_kwds(&mut self, t: &mut Tree, l: Id, frames: usize) -> Result<(), String> {
        let mut cur = next_split(t, l, -1, &REINDENT_SPLIT, self.stack, frames + 1)?;
        while let Some((mut tidx, _)) = cur {
            let prev = t.token_prev(l, Some(tidx), false, false);
            let breaks = prev.is_some_and(|(_, p)| matches!(last_char(t, p), Some('\n' | '\r')));
            if let Some((pidx, p)) = prev
                && t.n[p].is_whitespace
            {
                t.n[l].tokens.remove(pidx);
                tidx -= 1;
            }
            if !breaks {
                let nl = self.nl(t, 0)?;
                t.insert_before(l, tidx, nl);
                tidx += 1;
            }
            cur = next_split(t, l, tidx as isize, &REINDENT_SPLIT, self.stack, frames + 1)?;
        }
        Ok(())
    }

    fn split_statements(&mut self, t: &mut Tree, l: Id) -> Result<(), String> {
        let is_dml = |t: &Tree, id: Id| t.ttype_in(id, &[tt::DML, tt::DDL]);
        let mut cur = t.next_by(l, -1, is_dml);
        while let Some((mut tidx, _)) = cur {
            let prev = t.token_prev(l, Some(tidx), false, false);
            if let Some((pidx, p)) = prev
                && t.n[p].is_whitespace
            {
                t.n[l].tokens.remove(pidx);
                tidx -= 1;
            }
            if prev.is_some() {
                let nl = self.nl(t, 0)?;
                t.insert_before(l, tidx, nl);
                tidx += 1;
            }
            cur = t.next_by(l, tidx as isize, is_dml);
        }
        Ok(())
    }

    /// `_process`; `frames` counts the Python frames up to this call's.
    fn process(&mut self, t: &mut Tree, l: Id, frames: usize) -> Result<(), String> {
        match t.cls(l) {
            Cls::Where => self.process_where(t, l, frames + 1),
            Cls::Parenthesis => self.process_parenthesis(t, l, frames + 1),
            Cls::Function => {
                let first = *t.tokens(l).first().ok_or_else(index_error)?;
                self.last_func_len = Some(pystr::len(t.value(first)) as i128);
                self.process_default(t, l, true, frames + 2)
            }
            Cls::IdentifierList => self.process_identifierlist(t, l, frames + 1),
            Cls::Case => self.process_case(t, l, frames + 1),
            Cls::Values => self.process_values(t, l),
            _ => self.process_default(t, l, true, frames + 1),
        }
    }

    fn process_where(&mut self, t: &mut Tree, l: Id, frames: usize) -> Result<(), String> {
        let Some((tidx, _)) = t.next_by(l, -1, |t, id| t.matches(id, tt::KEYWORD, &["WHERE"]))
        else {
            return Ok(());
        };
        let nl = self.nl(t, 0)?;
        t.insert_before(l, tidx, nl);
        self.indent += 1;
        self.process_default(t, l, true, frames + 1)?;
        self.indent -= 1;
        Ok(())
    }

    fn process_parenthesis(&mut self, t: &mut Tree, l: Id, frames: usize) -> Result<(), String> {
        let is_dml_ddl = t
            .next_by(l, -1, |t, id| t.ttype_in(id, &[tt::DML, tt::DDL]))
            .is_some();
        let Some((_, first)) = t.next_by(l, -1, |t, id| t.matches(id, tt::PUNCTUATION, &["("]))
        else {
            return Ok(());
        };
        let n = i128::from(is_dml_ddl);
        self.indent += n;
        if is_dml_ddl {
            let nl = self.nl(t, 0)?;
            t.n[l].tokens.insert(0, nl);
        }
        let off = self.get_offset(t, first)? + 1;
        self.offset += off;
        self.process_default(t, l, !is_dml_ddl, frames + 1)?;
        self.offset -= off;
        self.indent -= n;
        Ok(())
    }

    fn process_identifierlist(&mut self, t: &mut Tree, l: Id, frames: usize) -> Result<(), String> {
        let mut identifiers: Vec<Id> = t
            .tokens(l)
            .iter()
            .copied()
            .filter(|&id| !(t.n[id].is_whitespace || t.matches(id, tt::PUNCTUATION, &[","])))
            .collect();
        let num_offset = if self.indent_columns {
            let id0 = *identifiers.first().ok_or_else(index_error)?;
            t.first_leaf(id0).ok_or_else(stop_iteration)?;
            if self.ch == '\t' { 1 } else { self.width }
        } else {
            if identifiers.is_empty() {
                return Err("IndexError: pop from empty list".into());
            }
            let id0 = identifiers.remove(0);
            let first = t.first_leaf(id0).ok_or_else(stop_iteration)?;
            if self.ch == '\t' {
                1
            } else {
                self.get_offset(t, first)?
            }
        };
        let vlen = |t: &Tree, id: Id| pystr::len(t.value(id)) as i128;
        if !t.within(l, Cls::Function) && !t.within(l, Cls::Values) {
            self.offset += num_offset;
            let mut position: i128 = 0;
            for &ident in &identifiers {
                position += vlen(t, ident) + 1;
                if position > self.wrap_after - self.offset {
                    let mut adjust = 0;
                    let mut token = ident;
                    if self.comma_first {
                        adjust = -2;
                        let idx = t.token_index(l, token)?;
                        let Some((_, comma)) = t.token_prev(l, Some(idx), true, false) else {
                            continue;
                        };
                        token = comma;
                    }
                    let nl = self.nl(t, adjust)?;
                    let idx = t.token_index(l, token)?;
                    t.insert_before(l, idx, nl);
                    if self.comma_first {
                        let idx = t.token_index(l, token)?;
                        if let Some((_, ws)) = t.token_next(l, Some(idx), false, false)
                            && !t.ttype_is(ws, tt::WHITESPACE)
                        {
                            let sp = t.new_token(tt::WHITESPACE, " ".into());
                            let idx = t.token_index(l, token)?;
                            t.insert_after(l, idx, sp, true);
                        }
                    }
                    position = 0;
                }
            }
            self.offset -= num_offset;
        } else {
            let mut i = 0;
            while let Some(&token) = t.tokens(l).get(i) {
                let next_ws = t.token_next(l, Some(i), false, false);
                if t.value(token) == "," {
                    match next_ws {
                        None => {
                            return Err("AttributeError: 'NoneType' object has no attribute 'is_whitespace'".into());
                        }
                        Some((_, ws)) if !t.n[ws].is_whitespace => {
                            let sp = t.new_token(tt::WHITESPACE, " ".into());
                            t.insert_after(l, i, sp, true);
                        }
                        _ => {}
                    }
                }
                i += 1;
            }
            let end_at = self.offset + identifiers.iter().map(|&id| vlen(t, id) + 1).sum::<i128>();
            let mut adjusted = 0;
            if self.wrap_after > 0
                && end_at > self.wrap_after - self.offset
                && let Some(len) = self.last_func_len
            {
                adjusted = -len - 1;
            }
            self.offset += adjusted;
            self.indent += 1;
            if adjusted < 0 {
                let nl = self.nl(t, 0)?;
                let first = *identifiers.first().ok_or_else(index_error)?;
                let idx = t.token_index(l, first)?;
                t.insert_before(l, idx, nl);
            }
            let mut position: i128 = 0;
            for &ident in &identifiers {
                position += vlen(t, ident) + 1;
                if self.wrap_after > 0 && position > self.wrap_after - self.offset {
                    let nl = self.nl(t, 0)?;
                    let idx = t.token_index(l, ident)?;
                    t.insert_before(l, idx, nl);
                    position = 0;
                }
            }
            self.indent -= 1;
            self.offset -= adjusted;
        }
        self.process_default(t, l, true, frames + 1)
    }

    fn process_case(&mut self, t: &mut Tree, l: Id, frames: usize) -> Result<(), String> {
        let mut cases = get_cases(t, l, false).into_iter();
        let (cond, _) = cases.next().ok_or_else(stop_iteration)?;
        let cond =
            cond.ok_or_else(|| "TypeError: 'NoneType' object is not subscriptable".to_string())?;
        let first = t
            .first_leaf(*cond.first().ok_or_else(index_error)?)
            .ok_or_else(stop_iteration)?;
        let head = *t.tokens(l).first().ok_or_else(index_error)?;
        let o1 = self.get_offset(t, head)?;
        self.offset += o1;
        let o2 = self.get_offset(t, first)?;
        self.offset += o2;
        for (cond, value) in cases {
            let cond_len: i128 = cond.iter().flatten().map(|&x| str_len(t, x)).sum();
            let value_len: i128 = value.iter().map(|&x| str_len(t, x)).sum();
            let end_pos = self.offset + 1 + cond_len + value_len;
            if !self.compact && end_pos > self.wrap_after {
                let token = match &cond {
                    None => *value.first().ok_or_else(index_error)?,
                    Some(c) => *c.first().ok_or_else(index_error)?,
                };
                let nl = self.nl(t, 0)?;
                let idx = t.token_index(l, token)?;
                t.insert_before(l, idx, nl);
            }
        }
        self.offset += 5;
        self.process_default(t, l, true, frames + 1)?;
        self.offset -= 5;
        self.offset -= o2;
        if let Some((end_idx, _)) = t.next_by(l, -1, |t, id| t.matches(id, tt::KEYWORD, &["END"]))
            && !self.compact
        {
            let nl = self.nl(t, 0)?;
            t.insert_before(l, end_idx, nl);
        }
        self.offset -= o1;
        Ok(())
    }

    fn process_values(&mut self, t: &mut Tree, l: Id) -> Result<(), String> {
        let nl = self.nl(t, 0)?;
        t.insert_before(l, 0, nl);
        let is_paren = |t: &Tree, id: Id| t.cls(id) == Cls::Parenthesis;
        let mut cur = t.next_by(l, -1, is_paren);
        let first_token = cur.map(|c| c.1);
        while let Some((tidx, token)) = cur {
            if let Some((_, ptoken)) = t.next_by(l, tidx as isize, |t, id| {
                t.matches(id, tt::PUNCTUATION, &[","])
            }) {
                if self.comma_first {
                    let off = self.get_offset(t, first_token.expect("set with the loop"))? - 2;
                    let nl = self.nl(t, off)?;
                    let idx = t.token_index(l, ptoken)?;
                    t.insert_before(l, idx, nl);
                } else {
                    let off = self.get_offset(t, token)?;
                    let nl = self.nl(t, off)?;
                    let idx = t.token_index(l, ptoken)?;
                    t.insert_after(l, idx, nl, true);
                }
            }
            cur = t.next_by(l, tidx as isize, is_paren);
        }
        Ok(())
    }

    fn process_default(
        &mut self,
        t: &mut Tree,
        l: Id,
        stmts: bool,
        frames: usize,
    ) -> Result<(), String> {
        if stmts {
            self.split_statements(t, l)?;
        }
        self.split_kwds(t, l, frames + 1)?;
        let mut i = 0;
        while let Some(&s) = t.tokens(l).get(i) {
            if t.is_group(s) {
                self.process(t, s, frames + 1)?;
            }
            i += 1;
        }
        Ok(())
    }

    /// `ReindentFilter.process(stmt)`; the caller reports how the statement ended with `finish`.
    pub(crate) fn run(&mut self, t: &mut Tree) -> Result<(), String> {
        // format, FilterStack.run, ReindentFilter.process, then _process, format being one level up as for
        // grouping
        self.process(t, ROOT, 5)?;
        if let Some(ends_nl) = self.last_stmt_nl {
            let nl = t.new_token(tt::WHITESPACE, if ends_nl { "\n" } else { "\n\n" }.into());
            t.n[ROOT].tokens.insert(0, nl);
        }
        Ok(())
    }

    /// Remembers `str(stmt).endswith('\n')` of the statement just done, for the next one's separator.
    pub(crate) fn finish(&mut self, ends_nl: bool) {
        self.last_stmt_nl = Some(ends_nl);
    }
}

// ---------------------------------------------------------------- AlignedIndentFilter

pub(crate) struct Aligned<'s> {
    ch: char,
    offset: i128,
    indent: i128,
    stack: &'s Stack,
}

const MAX_KWD_LEN: i128 = 6;

impl<'s> Aligned<'s> {
    pub(crate) fn new(tabs: bool, stack: &'s Stack) -> Aligned<'s> {
        Aligned {
            ch: if tabs { '\t' } else { ' ' },
            offset: 0,
            indent: 0,
            stack,
        }
    }

    fn nl(&self, t: &mut Tree, offset: i128) -> Result<Id, String> {
        let indent = self.indent * (2 + MAX_KWD_LEN);
        let v = format!(
            "\n{}",
            repeat(self.ch, MAX_KWD_LEN + offset + indent + self.offset)?
        );
        Ok(t.new_token(tt::WHITESPACE, v))
    }

    /// `nl(offset)` with a string offset: `-len(offset)`.
    fn nl_word(&self, t: &mut Tree, word: &str) -> Result<Id, String> {
        self.nl(t, -(pystr::len(word) as i128))
    }

    fn process(&mut self, t: &mut Tree, l: Id, frames: usize) -> Result<(), String> {
        match t.cls(l) {
            Cls::Statement => self.process_statement(t, l, frames + 1),
            Cls::Parenthesis => self.process_parenthesis(t, l, frames + 1),
            Cls::IdentifierList => self.process_identifierlist(t, l, frames + 1),
            Cls::Case => self.process_case(t, l),
            _ => self.process_default(t, l, frames + 1),
        }
    }

    fn process_statement(&mut self, t: &mut Tree, l: Id, frames: usize) -> Result<(), String> {
        if let Some(&first) = t.tokens(l).first()
            && t.n[first].is_whitespace
            && self.indent == 0
        {
            t.n[l].tokens.remove(0);
        }
        // self._process(sql.TokenList(tlist.tokens)): a TokenList over the same list, so _process_default
        self.process_default(t, l, frames + 2)
    }

    fn process_parenthesis(&mut self, t: &mut Tree, l: Id, frames: usize) -> Result<(), String> {
        if t.next_by(l, -1, |t, id| t.matches(id, tt::DML, &["SELECT"]))
            .is_none()
        {
            return Ok(());
        }
        self.indent += 1;
        let nl = self.nl_word(t, "SELECT")?;
        t.insert_after(l, 0, nl, true);
        self.process_default(t, l, frames + 1)?;
        self.indent -= 1;
        let nl = self.nl(t, 1)?;
        let last = t.tokens(l).len() - 1;
        t.insert_before(l, last, nl);
        Ok(())
    }

    fn process_identifierlist(&mut self, t: &mut Tree, l: Id, frames: usize) -> Result<(), String> {
        let mut identifiers: Vec<Id> = t
            .tokens(l)
            .iter()
            .copied()
            .filter(|&id| !(t.n[id].is_whitespace || t.matches(id, tt::PUNCTUATION, &[","])))
            .collect();
        if identifiers.is_empty() {
            return Err("IndexError: pop from empty list".into());
        }
        identifiers.remove(0);
        for token in identifiers {
            let nl = self.nl(t, 1)?;
            let idx = t.token_index(l, token)?;
            t.insert_before(l, idx, nl);
        }
        self.process_default(t, l, frames + 1)
    }

    fn process_case(&mut self, t: &mut Tree, l: Id) -> Result<(), String> {
        let offset_: i128 = 10;
        let mut cases = get_cases(t, l, true);
        if let Some((_, end)) = t.next_by(l, -1, |t, id| t.matches(id, tt::KEYWORD, &["END"])) {
            cases.push((None, vec![end]));
        }
        let widths: Vec<i128> = cases
            .iter()
            .map(|(cond, _)| match cond {
                Some(c) if !c.is_empty() => {
                    let parts: Vec<String> = c.iter().map(|&x| t.to_str(x)).collect();
                    pystr::len(&parts.join(" ")) as i128
                }
                _ => 0,
            })
            .collect();
        let max_width = widths.iter().copied().max().unwrap_or(0);
        for (i, (cond, value)) in cases.iter().enumerate() {
            let cond = cond.as_ref().filter(|c| !c.is_empty());
            let stmt = match cond {
                Some(c) => c[0],
                None => *value.first().ok_or_else(index_error)?,
            };
            if i > 0 {
                let nl = self.nl(t, offset_ - str_len(t, stmt))?;
                let idx = t.token_index(l, stmt)?;
                t.insert_before(l, idx, nl);
            }
            if let Some(c) = cond {
                let ws = t.new_token(tt::WHITESPACE, repeat(self.ch, max_width - widths[i])?);
                let idx = t.token_index(l, c[c.len() - 1])?;
                t.insert_after(l, idx, ws, true);
            }
        }
        Ok(())
    }

    fn split_kwds(&mut self, t: &mut Tree, l: Id, frames: usize) -> Result<(), String> {
        let mut cur = next_split(t, l, -1, &ALIGNED_SPLIT, self.stack, frames + 1)?;
        while let Some((mut tidx, token)) = cur {
            let word =
                if keyword_regex(t, token, &[Pat::Join]) || keyword_regex(t, token, &[Pat::By]) {
                    first_word(t.value(token))
                        .ok_or_else(index_error)?
                        .to_string()
                } else {
                    t.to_str(token)
                };
            let nl = self.nl_word(t, &word)?;
            let idx = t.token_index(l, token)?;
            t.insert_before(l, idx, nl);
            tidx += 1;
            cur = next_split(t, l, tidx as isize, &ALIGNED_SPLIT, self.stack, frames + 1)?;
        }
        Ok(())
    }

    fn process_default(&mut self, t: &mut Tree, l: Id, frames: usize) -> Result<(), String> {
        self.split_kwds(t, l, frames + 1)?;
        let mut i = 0;
        while let Some(&s) = t.tokens(l).get(i) {
            if t.is_group(s) {
                let idx = t.token_index(l, s)?;
                let by = t
                    .token_prev(l, Some(idx), true, false)
                    .is_some_and(|(_, p)| keyword_regex(t, p, &[Pat::By]));
                let off = if by { 3 } else { 0 };
                self.offset += off;
                self.process(t, s, frames + 1)?;
                self.offset -= off;
            }
            i += 1;
        }
        Ok(())
    }

    /// `AlignedIndentFilter.process(stmt)`.
    pub(crate) fn run(&mut self, t: &mut Tree) -> Result<(), String> {
        // format, FilterStack.run, AlignedIndentFilter.process, then _process (format one level up)
        self.process(t, ROOT, 5)
    }
}
