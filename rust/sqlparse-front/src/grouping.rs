//! Port of `sqlparse.engine.grouping` (sqlparse 0.6.1), function by function and in `group()`'s order.

use std::cell::Cell;

use super::pystr;
use super::tree::{Cls, Id, ROOT, Tree, tt};

const T_NUMERICAL: [&str; 3] = [tt::NUMBER, tt::NUMBER_INTEGER, tt::NUMBER_FLOAT];
const T_STRING: [&str; 3] = [tt::STRING, tt::STRING_SINGLE, tt::STRING_SYMBOL];
const T_NAME: [&str; 2] = [tt::NAME, tt::NAME_PLACEHOLDER];

const DELIMITED: [Cls; 6] = [
    Cls::Parenthesis,
    Cls::SquareBrackets,
    Cls::Case,
    Cls::If,
    Cls::For,
    Cls::Begin,
];

const WHERE_CLOSE: [&str; 10] = [
    "ORDER BY",
    "GROUP BY",
    "LIMIT",
    "UNION",
    "UNION ALL",
    "EXCEPT",
    "INTERSECT",
    "HAVING",
    "RETURNING",
    "INTO",
];

pub(crate) const RECURSION_ERROR: &str = "SQLParseError: Maximum recursion depth exceeded";

/// Python's recursion limit as it applies to one call of the front door.
///
/// `@recurse` passes have no depth limit of their own: deep enough nesting (about 980 BEGIN blocks) runs into
/// the interpreter's, which FilterStack reports as an SQLParseError; so do the indent filters' `_next_token` on
/// long BETWEEN chains. Where that happens depends on how deep the caller already is. `base` counts the
/// caller's Python frames, which is exact unless the caller's stack passes through C code (each such call
/// counts for Python too), so near the limit `probe` measures the real headroom once.
pub(crate) struct Stack {
    /// Python frames below sqlparse's entry point (the caller's and the front door's).
    base: usize,
    /// `sys.getrecursionlimit()`.
    limit: usize,
    /// The exact `base`, measured by asking Python.
    probe: Option<fn() -> Option<usize>>,
    exact: Cell<Option<usize>>,
}

/// How far below the limit counting frames is trusted: a caller would need this many C calls on its stack.
const MARGIN: usize = 256;

impl Stack {
    /// No limit: for callers outside Python.
    pub(crate) fn unlimited() -> Stack {
        Stack {
            base: 0,
            limit: usize::MAX,
            probe: None,
            exact: Cell::new(None),
        }
    }

    pub(crate) fn python(base: usize, limit: usize, probe: fn() -> Option<usize>) -> Stack {
        Stack {
            base,
            limit,
            probe: Some(probe),
            exact: Cell::new(None),
        }
    }

    /// Whether `frames` frames from sqlparse's entry point up (that one included) are more than Python allows.
    pub(crate) fn overflows(&self, frames: usize) -> bool {
        if self.base.saturating_add(frames).saturating_add(MARGIN) < self.limit {
            return false;
        }
        let base = match self.exact.get() {
            Some(b) => b,
            None => {
                let b = self.probe.and_then(|p| p()).unwrap_or(self.base);
                self.exact.set(Some(b));
                b
            }
        };
        base.saturating_add(frames) >= self.limit
    }
}

fn check_limits(t: &Tree, l: Id, depth: usize) -> Result<(), String> {
    if depth > 100 {
        return Err("SQLParseError: Maximum grouping depth exceeded (100).".into());
    }
    if t.tokens(l).len() > 10000 {
        return Err("SQLParseError: Maximum number of tokens exceeded (10000).".into());
    }
    Ok(())
}

fn cls_in(t: &Tree, id: Id, classes: &[Cls]) -> bool {
    classes.contains(&t.cls(id))
}

/// `M_OPEN` and `M_CLOSE` of the classes `_group_matching` builds.
fn open_close(
    cls: Cls,
) -> (
    &'static str,
    &'static [&'static str],
    &'static str,
    &'static [&'static str],
) {
    match cls {
        Cls::SquareBrackets => (tt::PUNCTUATION, &["["], tt::PUNCTUATION, &["]"]),
        Cls::Parenthesis => (tt::PUNCTUATION, &["("], tt::PUNCTUATION, &[")"]),
        Cls::Case => (tt::KEYWORD, &["CASE"], tt::KEYWORD, &["END"]),
        Cls::If => (tt::KEYWORD, &["IF"], tt::KEYWORD, &["END IF"]),
        Cls::For => (tt::KEYWORD, &["FOR", "FOREACH"], tt::KEYWORD, &["END LOOP"]),
        Cls::Begin => (tt::KEYWORD, &["BEGIN"], tt::KEYWORD, &["END"]),
        _ => unreachable!("not a matching group"),
    }
}

/// `_group_matching`.
fn group_matching(t: &mut Tree, l: Id, cls: Cls, depth: usize) -> Result<(), String> {
    check_limits(t, l, depth)?;
    let (open_t, open_v, close_t, close_v) = open_close(cls);
    let mut opens: Vec<usize> = Vec::new();
    let mut offset = 0usize;
    let snapshot = t.tokens(l).to_vec();
    for (idx, &token) in snapshot.iter().enumerate() {
        let tidx = idx - offset;
        if t.n[token].is_whitespace {
            continue;
        }
        if t.is_group(token) && t.cls(token) != cls {
            group_matching(t, token, cls, depth + 1)?;
            continue;
        }
        if t.matches(token, open_t, open_v) {
            opens.push(tidx);
        } else if t.matches(token, close_t, close_v) {
            let Some(open_idx) = opens.pop() else {
                continue;
            };
            t.group_tokens(l, cls, open_idx, tidx, true, false);
            offset += tidx - open_idx;
        }
    }
    Ok(())
}

/// `post(tlist, pidx, tidx, nidx)`: the range to group.
type Post<'a> = &'a dyn Fn(&mut Tree, Id, usize, usize, Option<usize>) -> (usize, usize);

/// What `_group` is given.
struct Spec<'a> {
    cls: Cls,
    is_match: &'a dyn Fn(&Tree, Id) -> bool,
    valid_prev: &'a dyn Fn(&Tree, Id) -> bool,
    valid_next: &'a dyn Fn(&Tree, Option<Id>) -> bool,
    post: Post<'a>,
    extend: bool,
}

fn is_delimiter(t: &Tree, l: Id, token: Option<Id>) -> bool {
    let Some(token) = token else { return false };
    cls_in(t, l, &DELIMITED)
        && (t.tokens(l).first() == Some(&token) || t.tokens(l).last() == Some(&token))
}

/// `_group`. Tokens are visited from a copy of the list taken at the start, as in Python, so a token already
/// moved into a group can still be looked at (with an index that is then off).
fn group_(t: &mut Tree, l: Id, s: &Spec, recurse: bool, depth: usize) -> Result<(), String> {
    check_limits(t, l, depth)?;
    let mut offset: isize = 0;
    let mut prev: Option<(usize, Id)> = None;
    let snapshot = t.tokens(l).to_vec();
    for (idx, &token) in snapshot.iter().enumerate() {
        let tidx = idx as isize - offset;
        if tidx < 0 {
            continue;
        }
        let tidx = tidx as usize;
        if t.n[token].is_whitespace {
            continue;
        }
        if recurse && t.is_group(token) && t.cls(token) != s.cls {
            group_(t, token, s, true, depth + 1)?;
        }
        if (s.is_match)(t, token) {
            let next = t.token_next(l, Some(tidx), true, false);
            if let Some((pidx, p)) = prev
                && (s.valid_prev)(t, p)
                && (s.valid_next)(t, next.map(|n| n.1))
                && !is_delimiter(t, l, Some(p))
                && !is_delimiter(t, l, next.map(|n| n.1))
            {
                let (from, to) = (s.post)(t, l, pidx, tidx, next.map(|n| n.0));
                let grp = t.group_tokens(l, s.cls, from, to, true, s.extend);
                offset += to as isize - from as isize;
                prev = Some((from, grp));
                continue;
            }
        }
        prev = Some((tidx, token));
    }
    Ok(())
}

fn always(_: &Tree, _: Id) -> bool {
    true
}

fn some(_: &Tree, token: Option<Id>) -> bool {
    token.is_some()
}

fn prev_next(_: &mut Tree, _: Id, pidx: usize, _: usize, nidx: Option<usize>) -> (usize, usize) {
    (pidx, nidx.expect("valid_next saw a token"))
}

fn tidx_next(_: &mut Tree, _: Id, _: usize, tidx: usize, nidx: Option<usize>) -> (usize, usize) {
    (tidx, nidx.expect("valid_next saw a token"))
}

fn group_typecasts(t: &mut Tree) -> Result<(), String> {
    let spec = Spec {
        cls: Cls::Identifier,
        is_match: &|t, id| t.matches(id, tt::PUNCTUATION, &["::"]),
        valid_prev: &always,
        valid_next: &some,
        post: &prev_next,
        extend: true,
    };
    group_(t, ROOT, &spec, true, 0)
}

fn group_tzcasts(t: &mut Tree) -> Result<(), String> {
    let spec = Spec {
        cls: Cls::Identifier,
        is_match: &|t, id| t.ttype_is(id, tt::TZCAST),
        valid_prev: &always,
        valid_next: &|t, id| {
            id.is_some_and(|id| {
                t.n[id].is_whitespace
                    || t.matches(id, tt::KEYWORD, &["AS"])
                    || t.ttype_is(id, tt::STRING_SINGLE)
            })
        },
        post: &prev_next,
        extend: true,
    };
    group_(t, ROOT, &spec, true, 0)
}

fn group_typed_literal(t: &mut Tree) -> Result<(), String> {
    let first = Spec {
        cls: Cls::TypedLiteral,
        is_match: &|t, id| {
            t.ttype_is(id, tt::NAME_BUILTIN) || t.matches(id, tt::KEYWORD, &["TIMESTAMP"])
        },
        valid_prev: &always,
        valid_next: &|t, id| id.is_some_and(|id| t.ttype_is(id, tt::STRING_SINGLE)),
        post: &tidx_next,
        extend: false,
    };
    group_(t, ROOT, &first, true, 0)?;
    let extend = Spec {
        cls: Cls::TypedLiteral,
        is_match: &|t, id| t.cls(id) == Cls::TypedLiteral,
        valid_prev: &always,
        valid_next: &|t, id| {
            id.is_some_and(|id| {
                t.matches(
                    id,
                    tt::KEYWORD,
                    &["DAY", "HOUR", "MINUTE", "MONTH", "SECOND", "YEAR"],
                )
            })
        },
        post: &tidx_next,
        extend: true,
    };
    group_(t, ROOT, &extend, true, 0)
}

fn group_period(t: &mut Tree) -> Result<(), String> {
    let spec = Spec {
        cls: Cls::Identifier,
        is_match: &|t, id| {
            t.matches(id, tt::PUNCTUATION, &["."])
                || t.matches(id, tt::OPERATOR, &["->"])
                || t.matches(id, tt::OPERATOR, &["->>"])
        },
        valid_prev: &|t, id| {
            cls_in(t, id, &[Cls::SquareBrackets, Cls::Identifier])
                || t.ttype_in(id, &[tt::NAME, tt::STRING_SYMBOL])
        },
        valid_next: &|_, _| true,
        post: &|t, l, pidx, tidx, nidx| {
            let valid = nidx.map(|i| t.tokens(l)[i]).is_some_and(|next| {
                cls_in(t, next, &[Cls::SquareBrackets, Cls::Function])
                    || t.ttype_in(
                        next,
                        &[tt::NAME, tt::STRING_SYMBOL, tt::WILDCARD, tt::STRING_SINGLE],
                    )
            });
            match nidx {
                Some(n) if valid => (pidx, n),
                _ => (pidx, tidx),
            }
        },
        extend: true,
    };
    group_(t, ROOT, &spec, true, 0)
}

fn group_as(t: &mut Tree) -> Result<(), String> {
    let spec = Spec {
        cls: Cls::Identifier,
        is_match: &|t, id| t.n[id].is_keyword && t.normalized(id) == "AS",
        valid_prev: &|t, id| t.normalized(id) == "NULL" || !t.n[id].is_keyword,
        valid_next: &|t, id| id.is_some_and(|id| !t.ttype_in(id, &[tt::DML, tt::DDL, tt::CTE])),
        post: &prev_next,
        extend: true,
    };
    group_(t, ROOT, &spec, true, 0)
}

fn group_assignment(t: &mut Tree) -> Result<(), String> {
    let spec = Spec {
        cls: Cls::Assignment,
        is_match: &|t, id| t.matches(id, tt::ASSIGNMENT, &[":="]),
        valid_prev: &|t, id| !t.ttype_in(id, &[tt::KEYWORD]),
        valid_next: &|t, id| id.is_some_and(|id| !t.ttype_in(id, &[tt::KEYWORD])),
        post: &|t, l, pidx, _, nidx| {
            let nidx = nidx.expect("valid_next saw a token");
            let semicolon = t.next_by(l, nidx as isize, |t, id| {
                t.matches(id, tt::PUNCTUATION, &[";"])
            });
            match semicolon {
                Some((s, _)) if s != 0 => (pidx, s),
                _ => (pidx, nidx),
            }
        },
        extend: true,
    };
    group_(t, ROOT, &spec, true, 0)
}

fn numerical_string_name(t: &Tree, id: Id) -> bool {
    t.ttype_in(id, &T_NUMERICAL) || t.ttype_in(id, &T_STRING) || t.ttype_in(id, &T_NAME)
}

fn group_comparison(t: &mut Tree) -> Result<(), String> {
    let valid = |t: &Tree, id: Id| {
        numerical_string_name(t, id)
            || cls_in(
                t,
                id,
                &[
                    Cls::Parenthesis,
                    Cls::Function,
                    Cls::Identifier,
                    Cls::Operation,
                    Cls::TypedLiteral,
                ],
            )
            || (t.n[id].is_keyword && t.normalized(id) == "NULL")
    };
    let spec = Spec {
        cls: Cls::Comparison,
        is_match: &|t, id| t.ttype_is(id, tt::COMPARISON),
        valid_prev: &valid,
        valid_next: &|t, id| id.is_some_and(|id| valid(t, id)),
        post: &prev_next,
        extend: false,
    };
    group_(t, ROOT, &spec, true, 0)
}

fn group_operator(t: &mut Tree) -> Result<(), String> {
    let valid = |t: &Tree, id: Id| {
        cls_in(
            t,
            id,
            &[
                Cls::SquareBrackets,
                Cls::Parenthesis,
                Cls::Function,
                Cls::Identifier,
                Cls::Operation,
                Cls::TypedLiteral,
            ],
        ) || numerical_string_name(t, id)
            || t.matches(
                id,
                tt::KEYWORD,
                &["CURRENT_DATE", "CURRENT_TIME", "CURRENT_TIMESTAMP"],
            )
    };
    let spec = Spec {
        cls: Cls::Operation,
        is_match: &|t, id| t.ttype_in(id, &[tt::OPERATOR, tt::WILDCARD]),
        valid_prev: &valid,
        valid_next: &|t, id| id.is_some_and(|id| valid(t, id)),
        post: &|t, l, pidx, tidx, nidx| {
            let token = t.tokens(l)[tidx];
            t.n[token].ttype = Some(tt::OPERATOR);
            (pidx, nidx.expect("valid_next saw a token"))
        },
        extend: false,
    };
    group_(t, ROOT, &spec, true, 0)
}

fn group_identifier_list(t: &mut Tree) -> Result<(), String> {
    let valid = |t: &Tree, id: Id| {
        cls_in(
            t,
            id,
            &[
                Cls::Function,
                Cls::Case,
                Cls::Identifier,
                Cls::Comparison,
                Cls::IdentifierList,
                Cls::Operation,
            ],
        ) || t.matches(id, tt::KEYWORD, &["null", "role"])
            || numerical_string_name(t, id)
            || t.ttype_in(id, &[tt::KEYWORD, tt::COMMENT, tt::WILDCARD])
    };
    let spec = Spec {
        cls: Cls::IdentifierList,
        is_match: &|t, id| t.matches(id, tt::PUNCTUATION, &[","]),
        valid_prev: &valid,
        valid_next: &|t, id| id.is_some_and(|id| valid(t, id)),
        post: &prev_next,
        extend: true,
    };
    group_(t, ROOT, &spec, true, 0)
}

fn group_arrays(t: &mut Tree) -> Result<(), String> {
    let spec = Spec {
        cls: Cls::Identifier,
        is_match: &|t, id| t.cls(id) == Cls::SquareBrackets,
        valid_prev: &|t, id| {
            cls_in(
                t,
                id,
                &[Cls::SquareBrackets, Cls::Identifier, Cls::Function],
            ) || t.ttype_in(id, &[tt::NAME, tt::STRING_SYMBOL])
        },
        valid_next: &|_, _| true,
        post: &|_, _, pidx, tidx, _| (pidx, tidx),
        extend: true,
    };
    group_(t, ROOT, &spec, false, 0)
}

/// What an `@recurse` pass is told about Python's stack: how many frames its body uses at most for a list
/// (above the `wrapped_f` frame), when its nesting can be deep enough to matter.
type Frames<'a> = Option<(&'a Stack, usize, &'a dyn Fn(&Tree, Id) -> usize)>;

/// `@recurse(*skip)`: the sublists that are not of a skipped class first, then the list itself.
fn recurse(
    t: &mut Tree,
    l: Id,
    skip: &[Cls],
    level: usize,
    frames: Frames,
    f: &dyn Fn(&mut Tree, Id) -> Result<(), String>,
) -> Result<(), String> {
    if let Some((stack, entry, body)) = frames {
        // sqlparse's entry point, FilterStack.run, group(), then one wrapped_f per level
        if stack.overflows(entry + level + 1 + body(t, l)) {
            return Err(RECURSION_ERROR.into());
        }
    }
    let mut i = 0;
    while let Some(&s) = t.tokens(l).get(i) {
        if t.is_group(s) && !skip.contains(&t.cls(s)) {
            recurse(t, s, skip, level + 1, frames, f)?;
        }
        i += 1;
    }
    f(t, l)
}

/// Frames a pass that looks for a keyword with `token_next_by(m=...)` uses: token_next_by, _token_matching,
/// the lambda, imt, Token.match and, for a token typed exactly Keyword, the generator of upper-cased values.
fn keyword_search_frames(t: &Tree, l: Id) -> usize {
    let toks = t.tokens(l);
    if toks.is_empty() {
        3
    } else if toks.iter().any(|&id| t.ttype_is(id, tt::KEYWORD)) {
        7
    } else {
        6
    }
}

fn group_comments(t: &mut Tree) -> Result<(), String> {
    recurse(t, ROOT, &[Cls::Comment], 0, None, &|t, l| {
        let is_comment = |t: &Tree, id: Id| t.ttype_is_a(id, tt::COMMENT);
        let mut cur = t.next_by(l, -1, is_comment);
        while let Some((tidx, _)) = cur {
            let end = t.find(l, tidx as isize, |t, id| {
                !(t.ttype_is_a(id, tt::COMMENT) || t.n[id].is_newline)
            });
            let Some((eidx, _)) = end else { break };
            let (eidx, _) = t
                .token_prev(l, Some(eidx), false, false)
                .expect("the comment itself comes before");
            t.group_tokens(l, Cls::Comment, tidx, eidx, true, false);
            cur = t.next_by(l, tidx as isize, is_comment);
        }
        Ok(())
    })
}

fn group_over(t: &mut Tree, stack: &Stack, entry: usize) -> Result<(), String> {
    recurse(
        t,
        ROOT,
        &[Cls::Over],
        0,
        Some((stack, entry, &keyword_search_frames)),
        &|t, l| {
            let is_over = |t: &Tree, id: Id| t.matches(id, tt::KEYWORD, &["OVER"]);
            let mut cur = t.next_by(l, -1, is_over);
            while let Some((tidx, _)) = cur {
                if let Some((nidx, next)) = t.token_next(l, Some(tidx), true, false)
                    && (t.cls(next) == Cls::Parenthesis || t.ttype_is_a(next, tt::NAME))
                {
                    t.group_tokens(l, Cls::Over, tidx, nidx, true, false);
                }
                cur = t.next_by(l, tidx as isize, is_over);
            }
            Ok(())
        },
    )
}

fn create_table_without_as(t: &Tree, l: Id) -> bool {
    let (mut create, mut table, mut as_) = (false, false, false);
    for &id in t.tokens(l) {
        let v = t.value(id);
        // `value.upper() == 'CREATE'`: only a value of the same length (or one that grows when upper-cased,
        // which needs a non-ASCII character) can match
        if v.len() > 6 && v.is_ascii() {
            continue;
        }
        let up = pystr::upper(v);
        match up.as_str() {
            "CREATE" => create = true,
            "TABLE" => table = true,
            "AS" => as_ = true,
            _ => {}
        }
    }
    create && table && !as_
}

fn group_functions(t: &mut Tree, stack: &Stack, entry: usize) -> Result<(), String> {
    let frames = |t: &Tree, l: Id| {
        if create_table_without_as(t, l) {
            1
        } else if t.tokens(l).is_empty() {
            3
        } else {
            // token_next_by(t=T.Name): ..., imt, _TokenType.__contains__
            6
        }
    };
    recurse(
        t,
        ROOT,
        &[Cls::Function],
        0,
        Some((stack, entry, &frames)),
        &|t, l| {
            if create_table_without_as(t, l) {
                return Ok(());
            }
            let is_name = |t: &Tree, id: Id| t.ttype_is_a(id, tt::NAME);
            let mut cur = t.next_by(l, -1, is_name);
            while let Some((tidx, _)) = cur {
                if let Some((nidx, next)) = t.token_next(l, Some(tidx), true, false)
                    && t.cls(next) == Cls::Parenthesis
                {
                    let eidx = match t.token_next(l, Some(nidx), true, false) {
                        Some((over_idx, over)) if t.cls(over) == Cls::Over => over_idx,
                        _ => nidx,
                    };
                    t.group_tokens(l, Cls::Function, tidx, eidx, true, false);
                }
                cur = t.next_by(l, tidx as isize, is_name);
            }
            Ok(())
        },
    )
}

/// `tlist._groupable_tokens[-1]`.
fn last_groupable(t: &Tree, l: Id) -> Result<Id, String> {
    let toks = t.tokens(l);
    let groupable = match t.cls(l) {
        Cls::Parenthesis | Cls::SquareBrackets if toks.len() >= 2 => &toks[1..toks.len() - 1],
        Cls::Parenthesis | Cls::SquareBrackets => &toks[0..0],
        _ => toks,
    };
    groupable
        .last()
        .copied()
        .ok_or_else(|| "IndexError: list index out of range".to_string())
}

fn group_where(t: &mut Tree, stack: &Stack, entry: usize) -> Result<(), String> {
    recurse(
        t,
        ROOT,
        &[Cls::Where],
        0,
        Some((stack, entry, &keyword_search_frames)),
        &|t, l| {
            let is_where = |t: &Tree, id: Id| t.matches(id, tt::KEYWORD, &["WHERE"]);
            let mut cur = t.next_by(l, -1, is_where);
            while let Some((tidx, _)) = cur {
                let close = t.next_by(l, tidx as isize, |t, id| {
                    t.matches(id, tt::KEYWORD, &WHERE_CLOSE)
                });
                let end = match close {
                    None => last_groupable(t, l)?,
                    Some((eidx, _)) => t.tokens(l)[eidx - 1],
                };
                let eidx = t.token_index(l, end)?;
                t.group_tokens(l, Cls::Where, tidx, eidx, true, false);
                cur = t.next_by(l, tidx as isize, is_where);
            }
            Ok(())
        },
    )
}

fn group_identifier(t: &mut Tree) -> Result<(), String> {
    recurse(t, ROOT, &[Cls::Identifier], 0, None, &|t, l| {
        let is_name = |t: &Tree, id: Id| t.ttype_in(id, &[tt::STRING_SYMBOL, tt::NAME]);
        let mut cur = t.next_by(l, -1, is_name);
        while let Some((tidx, _)) = cur {
            t.group_tokens(l, Cls::Identifier, tidx, tidx, true, false);
            cur = t.next_by(l, tidx as isize, is_name);
        }
        Ok(())
    })
}

fn group_order(t: &mut Tree) -> Result<(), String> {
    recurse(t, ROOT, &[Cls::Identifier], 0, None, &|t, l| {
        let is_order = |t: &Tree, id: Id| t.ttype_is_a(id, tt::ORDER);
        let mut cur = t.next_by(l, -1, is_order);
        while let Some((mut tidx, _)) = cur {
            if let Some((pidx, prev)) = t.token_prev(l, Some(tidx), true, false)
                && (t.cls(prev) == Cls::Identifier || t.ttype_is_a(prev, tt::NUMBER))
            {
                t.group_tokens(l, Cls::Identifier, pidx, tidx, true, false);
                tidx = pidx;
            }
            cur = t.next_by(l, tidx as isize, is_order);
        }
        Ok(())
    })
}

fn group_aliased(t: &mut Tree) -> Result<(), String> {
    recurse(t, ROOT, &[], 0, None, &|t, l| {
        let is_aliased = |t: &Tree, id: Id| {
            cls_in(
                t,
                id,
                &[
                    Cls::Parenthesis,
                    Cls::Function,
                    Cls::Case,
                    Cls::Identifier,
                    Cls::Operation,
                    Cls::Comparison,
                ],
            ) || t.ttype_is_a(id, tt::NUMBER)
        };
        let mut cur = t.next_by(l, -1, is_aliased);
        while let Some((tidx, _)) = cur {
            if let Some((nidx, next)) = t.token_next(l, Some(tidx), true, false)
                && t.cls(next) == Cls::Identifier
            {
                t.group_tokens(l, Cls::Identifier, tidx, nidx, true, true);
            }
            cur = t.next_by(l, tidx as isize, is_aliased);
        }
        Ok(())
    })
}

fn align_comments(t: &mut Tree) -> Result<(), String> {
    recurse(t, ROOT, &[], 0, None, &|t, l| {
        let is_comment = |t: &Tree, id: Id| t.cls(id) == Cls::Comment;
        let mut cur = t.next_by(l, -1, is_comment);
        while let Some((mut tidx, _)) = cur {
            if let Some((pidx, prev)) = t.token_prev(l, Some(tidx), true, false)
                && t.is_group(prev)
            {
                t.group_tokens(l, Cls::TokenList, pidx, tidx, true, true);
                tidx = pidx;
            }
            cur = t.next_by(l, tidx as isize, is_comment);
        }
        Ok(())
    })
}

fn group_values(t: &mut Tree) {
    let l = ROOT;
    let mut cur = t.next_by(l, -1, |t, id| t.matches(id, tt::KEYWORD, &["VALUES"]));
    let start_idx = cur.map(|c| c.0);
    let mut end_idx = None;
    while let Some((tidx, token)) = cur {
        if t.cls(token) == Cls::Parenthesis {
            end_idx = Some(tidx);
        }
        cur = t.token_next(l, Some(tidx), true, false);
    }
    if let (Some(start), Some(end)) = (start_idx, end_idx) {
        t.group_tokens(l, Cls::Values, start, end, true, true);
    }
}

/// `grouping.group(stmt)`. `entry` is how many levels Python's recursion counter is up when grouping starts,
/// from sqlparse's entry point: 3 for `parse`, 4 for `format` (measured with tools/check_recursion.py).
pub(crate) fn group(t: &mut Tree, stack: &Stack, entry: usize) -> Result<(), String> {
    group_comments(t)?;
    for cls in [
        Cls::SquareBrackets,
        Cls::Parenthesis,
        Cls::Case,
        Cls::If,
        Cls::For,
        Cls::Begin,
    ] {
        group_matching(t, ROOT, cls, 0)?;
    }
    group_over(t, stack, entry)?;
    group_functions(t, stack, entry)?;
    group_where(t, stack, entry)?;
    group_period(t)?;
    group_arrays(t)?;
    group_identifier(t)?;
    group_order(t)?;
    group_typecasts(t)?;
    group_tzcasts(t)?;
    group_typed_literal(t)?;
    group_operator(t)?;
    group_comparison(t)?;
    group_as(t)?;
    group_aliased(t)?;
    group_assignment(t)?;
    align_comments(t)?;
    group_identifier_list(t)?;
    group_values(t);
    Ok(())
}
