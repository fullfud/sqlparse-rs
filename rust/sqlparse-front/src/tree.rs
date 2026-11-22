//! Port of `sqlparse.sql` (sqlparse 0.6.1): the statement tree.
//!
//! Nodes live in one arena per statement. A node keeps what a Python `Token` / `TokenList` keeps, with the
//! same quirks:
//! - a group's `value` is `''.join(child.value)` taken when the group is made, extended by `group_tokens(...,
//!   extend=True)`, and never refreshed after that (filters that change leaves leave it stale); `str()` of a
//!   group is recomputed from the leaves;
//! - `normalized` is taken at creation (`value.upper()` for keywords) and does not follow later value changes;
//! - `is_keyword`, `is_whitespace`, `is_newline` come from the type at creation (`group_operator` changes a
//!   token's type afterwards);
//! - `parent` is set where Python sets it and left alone where Python leaves it (tokens put in with
//!   `tokens.insert` have none).
//!
//! While grouping runs, every value is a slice of the statement's text, so values are kept as spans of it
//! until a filter gives a node a new one.

use super::pystr;
use super::split::is_a;

pub(crate) type Id = usize;

/// The node's Python class.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Cls {
    Token,
    TokenList,
    Statement,
    Identifier,
    IdentifierList,
    TypedLiteral,
    Parenthesis,
    SquareBrackets,
    Assignment,
    If,
    For,
    Comparison,
    Comment,
    Where,
    Over,
    Case,
    Function,
    Begin,
    Operation,
    Values,
}

impl Cls {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Cls::Token => "Token",
            Cls::TokenList => "TokenList",
            Cls::Statement => "Statement",
            Cls::Identifier => "Identifier",
            Cls::IdentifierList => "IdentifierList",
            Cls::TypedLiteral => "TypedLiteral",
            Cls::Parenthesis => "Parenthesis",
            Cls::SquareBrackets => "SquareBrackets",
            Cls::Assignment => "Assignment",
            Cls::If => "If",
            Cls::For => "For",
            Cls::Comparison => "Comparison",
            Cls::Comment => "Comment",
            Cls::Where => "Where",
            Cls::Over => "Over",
            Cls::Case => "Case",
            Cls::Function => "Function",
            Cls::Begin => "Begin",
            Cls::Operation => "Operation",
            Cls::Values => "Values",
        }
    }
}

/// Token types, as `str(ttype)` spells them.
pub(crate) mod tt {
    pub(crate) const WHITESPACE: &str = "Token.Text.Whitespace";
    pub(crate) const NEWLINE: &str = "Token.Text.Whitespace.Newline";
    pub(crate) const KEYWORD: &str = "Token.Keyword";
    pub(crate) const DML: &str = "Token.Keyword.DML";
    pub(crate) const DDL: &str = "Token.Keyword.DDL";
    pub(crate) const CTE: &str = "Token.Keyword.CTE";
    pub(crate) const ORDER: &str = "Token.Keyword.Order";
    pub(crate) const TZCAST: &str = "Token.Keyword.TZCast";
    pub(crate) const NAME: &str = "Token.Name";
    pub(crate) const NAME_BUILTIN: &str = "Token.Name.Builtin";
    pub(crate) const NAME_PLACEHOLDER: &str = "Token.Name.Placeholder";
    pub(crate) const STRING: &str = "Token.Literal.String";
    pub(crate) const STRING_SINGLE: &str = "Token.Literal.String.Single";
    pub(crate) const STRING_SYMBOL: &str = "Token.Literal.String.Symbol";
    pub(crate) const NUMBER: &str = "Token.Literal.Number";
    pub(crate) const NUMBER_INTEGER: &str = "Token.Literal.Number.Integer";
    pub(crate) const NUMBER_FLOAT: &str = "Token.Literal.Number.Float";
    pub(crate) const PUNCTUATION: &str = "Token.Punctuation";
    pub(crate) const OPERATOR: &str = "Token.Operator";
    pub(crate) const COMPARISON: &str = "Token.Operator.Comparison";
    pub(crate) const WILDCARD: &str = "Token.Wildcard";
    pub(crate) const COMMENT: &str = "Token.Comment";
    pub(crate) const MULTILINE_HINT: &str = "Token.Comment.Multiline.Hint";
    pub(crate) const SINGLE_HINT: &str = "Token.Comment.Single.Hint";
    pub(crate) const ASSIGNMENT: &str = "Token.Assignment";
}

#[derive(Clone)]
enum Val {
    /// A byte range of the statement's text.
    Span(u32, u32),
    Owned(String),
}

pub(crate) struct Node {
    pub(crate) cls: Cls,
    pub(crate) ttype: Option<&'static str>,
    val: Val,
    /// `normalized` where it is not the value the node was made with.
    norm: Option<String>,
    /// The node's value changed since it was made (so `norm` holds the original `normalized`).
    changed: bool,
    pub(crate) is_keyword: bool,
    pub(crate) is_whitespace: bool,
    pub(crate) is_newline: bool,
    pub(crate) parent: Option<Id>,
    pub(crate) tokens: Vec<Id>,
}

pub(crate) struct Tree {
    text: String,
    pub(crate) n: Vec<Node>,
}

/// The statement, root of every tree.
pub(crate) const ROOT: Id = 0;

impl Tree {
    /// `sql.Statement([sql.Token(ttype, value), ...])` for one statement of the splitter.
    pub(crate) fn statement(tokens: &[(&'static str, String)]) -> Tree {
        let size: usize = tokens.iter().map(|(_, v)| v.len()).sum();
        let mut text = String::with_capacity(size);
        let mut tree = Tree {
            text: String::new(),
            n: Vec::with_capacity(tokens.len() * 2 + 1),
        };
        tree.n
            .push(Self::group_node(Cls::Statement, Val::Span(0, size as u32)));
        let mut children = Vec::with_capacity(tokens.len());
        for (ttype, value) in tokens {
            let start = text.len() as u32;
            text.push_str(value);
            let node = Self::leaf_node(ttype, Val::Span(start, text.len() as u32), value);
            let id = tree.n.len();
            tree.n.push(node);
            tree.n[id].parent = Some(ROOT);
            children.push(id);
        }
        tree.n[ROOT].tokens = children;
        tree.text = text;
        tree
    }

    fn leaf_node(ttype: &'static str, val: Val, value: &str) -> Node {
        let is_keyword = is_a(ttype, tt::KEYWORD);
        let norm = if is_keyword {
            let up = pystr::upper(value);
            (up != value).then_some(up)
        } else {
            None
        };
        Node {
            cls: Cls::Token,
            ttype: Some(ttype),
            val,
            norm,
            changed: false,
            is_keyword,
            is_whitespace: is_a(ttype, tt::WHITESPACE),
            is_newline: is_a(ttype, tt::NEWLINE),
            parent: None,
            tokens: Vec::new(),
        }
    }

    fn group_node(cls: Cls, val: Val) -> Node {
        Node {
            cls,
            ttype: None,
            val,
            norm: None,
            changed: false,
            is_keyword: false,
            is_whitespace: false,
            is_newline: false,
            parent: None,
            tokens: Vec::new(),
        }
    }

    /// A new `sql.Token(ttype, value)` (no parent yet).
    pub(crate) fn new_token(&mut self, ttype: &'static str, value: String) -> Id {
        let node = Self::leaf_node(ttype, Val::Owned(String::new()), &value);
        let id = self.n.len();
        self.n.push(node);
        self.n[id].val = Val::Owned(value);
        id
    }

    /// `grp_cls(tokens)`: the group's value is joined now, the children's parent becomes the group.
    pub(crate) fn new_group(&mut self, cls: Cls, tokens: Vec<Id>) -> Id {
        let val = self.joined(&tokens);
        let id = self.n.len();
        self.n.push(Self::group_node(cls, val));
        for &c in &tokens {
            self.n[c].parent = Some(id);
        }
        self.n[id].tokens = tokens;
        id
    }

    /// `''.join(token.value for token in tokens)`, as a span when the values are adjacent spans.
    fn joined(&self, tokens: &[Id]) -> Val {
        let mut span: Option<(u32, u32)> = None;
        let mut adjacent = true;
        for &c in tokens {
            match (&self.n[c].val, span) {
                (Val::Span(a, b), None) => span = Some((*a, *b)),
                (Val::Span(a, b), Some((s, e))) if *a == e => span = Some((s, *b)),
                (Val::Span(a, b), Some(_)) if a == b => {}
                (Val::Owned(v), _) if v.is_empty() => {}
                _ => {
                    adjacent = false;
                    break;
                }
            }
        }
        match (adjacent, span) {
            (true, Some((s, e))) => Val::Span(s, e),
            (true, None) => Val::Owned(String::new()),
            (false, _) => Val::Owned(tokens.iter().map(|&c| self.value(c)).collect()),
        }
    }

    pub(crate) fn value(&self, id: Id) -> &str {
        match &self.n[id].val {
            Val::Span(a, b) => &self.text[*a as usize..*b as usize],
            Val::Owned(s) => s,
        }
    }

    fn keep_normalized(&mut self, id: Id) {
        if !self.n[id].changed {
            if self.n[id].norm.is_none() {
                self.n[id].norm = Some(self.value(id).to_string());
            }
            self.n[id].changed = true;
        }
    }

    /// `token.value = value`.
    pub(crate) fn set_value(&mut self, id: Id, value: String) {
        self.keep_normalized(id);
        self.n[id].val = Val::Owned(value);
    }

    /// `grp.value += ''.join(token.value for token in tokens)`.
    fn append_value(&mut self, id: Id, tokens: &[Id]) {
        let more = self.joined(tokens);
        let merged = match (&self.n[id].val, &more) {
            (_, Val::Owned(s)) if s.is_empty() => return,
            (Val::Span(a, b), Val::Span(c, d)) if b == c => Val::Span(*a, *d),
            _ => {
                let mut s = self.value(id).to_string();
                match &more {
                    Val::Span(c, d) => s.push_str(&self.text[*c as usize..*d as usize]),
                    Val::Owned(o) => s.push_str(o),
                }
                Val::Owned(s)
            }
        };
        self.keep_normalized(id);
        self.n[id].val = merged;
    }

    pub(crate) fn normalized(&self, id: Id) -> &str {
        match &self.n[id].norm {
            Some(s) => s,
            None => self.value(id),
        }
    }

    pub(crate) fn is_group(&self, id: Id) -> bool {
        self.n[id].cls != Cls::Token
    }

    pub(crate) fn cls(&self, id: Id) -> Cls {
        self.n[id].cls
    }

    pub(crate) fn tokens(&self, id: Id) -> &[Id] {
        &self.n[id].tokens
    }

    /// `token.ttype is t` (and `token.ttype == t`).
    pub(crate) fn ttype_is(&self, id: Id, t: &str) -> bool {
        self.n[id].ttype == Some(t)
    }

    /// `token.ttype in (a, b, ...)` with a plain tuple: equality.
    pub(crate) fn ttype_in(&self, id: Id, ts: &[&str]) -> bool {
        self.n[id].ttype.is_some_and(|t| ts.contains(&t))
    }

    /// `token.ttype in T.X`: X or a subtype of it.
    pub(crate) fn ttype_is_a(&self, id: Id, parent: &str) -> bool {
        self.n[id].ttype.is_some_and(|t| is_a(t, parent))
    }

    /// `token.match(ttype, values)` with a list of values (`values=None` is `ttype_is`).
    pub(crate) fn matches(&self, id: Id, ttype: &str, values: &[&str]) -> bool {
        if !self.ttype_is(id, ttype) {
            return false;
        }
        let norm = self.normalized(id);
        if self.n[id].is_keyword {
            // the values are ASCII constants, so their str.upper() is the ASCII one
            values.iter().any(|v| {
                v.len() == norm.len()
                    && v.bytes()
                        .zip(norm.bytes())
                        .all(|(a, b)| a.to_ascii_uppercase() == b)
            })
        } else {
            values.contains(&norm)
        }
    }

    /// `str(token)`.
    pub(crate) fn to_str(&self, id: Id) -> String {
        let mut out = String::new();
        self.write_str(id, &mut out);
        out
    }

    pub(crate) fn write_str(&self, id: Id, out: &mut String) {
        if !self.is_group(id) {
            out.push_str(self.value(id));
            return;
        }
        let mut stack: Vec<(Id, usize)> = vec![(id, 0)];
        while let Some((g, i)) = stack.pop() {
            if let Some(&c) = self.n[g].tokens.get(i) {
                stack.push((g, i + 1));
                if self.is_group(c) {
                    stack.push((c, 0));
                } else {
                    out.push_str(self.value(c));
                }
            }
        }
    }

    /// The leaves of `token.flatten()`, in order.
    pub(crate) fn flatten(&self, id: Id) -> Vec<Id> {
        let mut out = Vec::new();
        if !self.is_group(id) {
            out.push(id);
            return out;
        }
        let mut stack: Vec<(Id, usize)> = vec![(id, 0)];
        while let Some((g, i)) = stack.pop() {
            if let Some(&c) = self.n[g].tokens.get(i) {
                stack.push((g, i + 1));
                if self.is_group(c) {
                    stack.push((c, 0));
                } else {
                    out.push(c);
                }
            }
        }
        out
    }

    /// `next(token.flatten())`: None where Python raises StopIteration.
    pub(crate) fn first_leaf(&self, id: Id) -> Option<Id> {
        if !self.is_group(id) {
            return Some(id);
        }
        let mut stack: Vec<(Id, usize)> = vec![(id, 0)];
        while let Some((g, i)) = stack.pop() {
            if let Some(&c) = self.n[g].tokens.get(i) {
                stack.push((g, i + 1));
                if self.is_group(c) {
                    stack.push((c, 0));
                } else {
                    return Some(c);
                }
            }
        }
        None
    }

    /// `_token_matching` forward: the first index in `start..len` (Python indexing, so a negative start counts
    /// from the end) whose token passes `f`.
    pub(crate) fn find(
        &self,
        l: Id,
        start: isize,
        f: impl Fn(&Tree, Id) -> bool,
    ) -> Option<(usize, Id)> {
        let toks = &self.n[l].tokens;
        let len = toks.len() as isize;
        let mut i = start;
        while i < len {
            let real = if i < 0 { i + len } else { i };
            if real >= 0 {
                let id = toks[real as usize];
                if f(self, id) {
                    return Some((real as usize, id));
                }
            }
            i += 1;
        }
        None
    }

    /// `tlist.token_next_by(..., idx=idx)` with the matcher `f`.
    pub(crate) fn next_by(
        &self,
        l: Id,
        idx: isize,
        f: impl Fn(&Tree, Id) -> bool,
    ) -> Option<(usize, Id)> {
        self.find(l, idx + 1, f)
    }

    fn skipped(&self, id: Id, skip_ws: bool, skip_cm: bool) -> bool {
        (skip_ws && self.n[id].is_whitespace)
            || (skip_cm && (self.cls(id) == Cls::Comment || self.ttype_is_a(id, tt::COMMENT)))
    }

    /// `tlist.token_next(idx, skip_ws, skip_cm)`.
    pub(crate) fn token_next(
        &self,
        l: Id,
        idx: Option<usize>,
        skip_ws: bool,
        skip_cm: bool,
    ) -> Option<(usize, Id)> {
        let idx = idx?;
        self.find(l, idx as isize + 1, |t, id| {
            !t.skipped(id, skip_ws, skip_cm)
        })
    }

    /// `tlist.token_prev(idx, skip_ws, skip_cm)`: looks at `idx - 1` down to 0.
    pub(crate) fn token_prev(
        &self,
        l: Id,
        idx: Option<usize>,
        skip_ws: bool,
        skip_cm: bool,
    ) -> Option<(usize, Id)> {
        let idx = idx?;
        let toks = &self.n[l].tokens;
        (0..idx.min(toks.len() + 1))
            .rev()
            .map(|i| (i, toks[i]))
            .find(|&(_, id)| !self.skipped(id, skip_ws, skip_cm))
    }

    /// `tlist.token_index(token)`.
    pub(crate) fn token_index(&self, l: Id, token: Id) -> Result<usize, String> {
        self.n[l]
            .tokens
            .iter()
            .position(|&t| t == token)
            .ok_or_else(|| "ValueError: token is not in list".to_string())
    }

    /// `tlist.group_tokens(grp_cls, start, end, include_end, extend)`.
    pub(crate) fn group_tokens(
        &mut self,
        l: Id,
        cls: Cls,
        start: usize,
        end: usize,
        include_end: bool,
        extend: bool,
    ) -> Id {
        let first = self.n[l].tokens[start];
        let len = self.n[l].tokens.len();
        let end_idx = (end + usize::from(include_end)).min(len);
        let same = if cls == Cls::TokenList {
            self.is_group(first)
        } else {
            self.cls(first) == cls
        };
        if extend && same {
            let hi = end_idx.max(start + 1);
            let sub: Vec<Id> = self.n[l].tokens.drain(start + 1..hi).collect();
            self.append_value(first, &sub);
            for &c in &sub {
                self.n[c].parent = Some(first);
            }
            self.n[first].tokens.extend(sub);
            first
        } else {
            let hi = end_idx.max(start);
            let sub: Vec<Id> = self.n[l].tokens[start..hi].to_vec();
            let grp = self.new_group(cls, sub);
            self.n[l].tokens.splice(start..hi, [grp]);
            self.n[grp].parent = Some(l);
            grp
        }
    }

    /// `tlist.insert_before(where, token)` with an index.
    pub(crate) fn insert_before(&mut self, l: Id, idx: usize, token: Id) {
        self.n[token].parent = Some(l);
        let idx = idx.min(self.n[l].tokens.len());
        self.n[l].tokens.insert(idx, token);
    }

    /// `tlist.insert_after(where, token, skip_ws)` with an index.
    pub(crate) fn insert_after(&mut self, l: Id, idx: usize, token: Id, skip_ws: bool) {
        let next = self.token_next(l, Some(idx), skip_ws, false);
        self.n[token].parent = Some(l);
        match next {
            None => self.n[l].tokens.push(token),
            Some((nidx, _)) => self.n[l].tokens.insert(nidx, token),
        }
    }

    /// `token.within(group_cls)`.
    pub(crate) fn within(&self, id: Id, cls: Cls) -> bool {
        let mut p = self.n[id].parent;
        while let Some(g) = p {
            if self.cls(g) == cls {
                return true;
            }
            p = self.n[g].parent;
        }
        false
    }
}
