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

/// A token type: its name as `str(ttype)` spells it, and its number in `tt::ALL` (types are compared by
/// number, and `ttype in T.X` is a bit in `tt::ANCESTORS`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct TT {
    pub(crate) name: &'static str,
    pub(crate) id: u8,
}

const fn same_bytes(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// `ttype in parent` for two names, at compile time.
const fn const_is_a(ttype: &[u8], parent: &[u8]) -> bool {
    if ttype.len() < parent.len() {
        return false;
    }
    let mut i = 0;
    while i < parent.len() {
        if ttype[i] != parent[i] {
            return false;
        }
        i += 1;
    }
    ttype.len() == parent.len() || ttype[parent.len()] == b'.'
}

impl TT {
    /// The type of that name, which must be in `tt::ALL` (a compile error otherwise).
    pub(crate) const fn named(name: &'static str) -> TT {
        let mut i = 0;
        while i < tt::ALL.len() {
            if same_bytes(tt::ALL[i].as_bytes(), name.as_bytes()) {
                return TT { name, id: i as u8 };
            }
            i += 1;
        }
        panic!("token type not in tt::ALL")
    }
}

/// Token types, as `str(ttype)` spells them.
pub(crate) mod tt {
    use super::TT;

    /// Every type the lexer makes and the grouping and filters name, in order of their numbers.
    pub(crate) const ALL: [&str; 32] = [
        "Token.Assignment",
        "Token.Comment",
        "Token.Comment.Multiline",
        "Token.Comment.Multiline.Hint",
        "Token.Comment.Single",
        "Token.Comment.Single.Hint",
        "Token.Error",
        "Token.Generic.Command",
        "Token.Keyword",
        "Token.Keyword.CTE",
        "Token.Keyword.DCL",
        "Token.Keyword.DDL",
        "Token.Keyword.DML",
        "Token.Keyword.Order",
        "Token.Keyword.TZCast",
        "Token.Literal",
        "Token.Literal.Number",
        "Token.Literal.Number.Float",
        "Token.Literal.Number.Hexadecimal",
        "Token.Literal.Number.Integer",
        "Token.Literal.String",
        "Token.Literal.String.Single",
        "Token.Literal.String.Symbol",
        "Token.Name",
        "Token.Name.Builtin",
        "Token.Name.Placeholder",
        "Token.Operator",
        "Token.Operator.Comparison",
        "Token.Punctuation",
        "Token.Text.Whitespace",
        "Token.Text.Whitespace.Newline",
        "Token.Wildcard",
    ];

    /// For each type, the types it is in (`ttype in T.X`): bit j of ANCESTORS[i] for ALL[j] a parent of ALL[i]
    /// or ALL[i] itself.
    pub(crate) const ANCESTORS: [u64; 32] = {
        let mut out = [0u64; 32];
        let mut i = 0;
        while i < ALL.len() {
            let mut j = 0;
            while j < ALL.len() {
                if super::const_is_a(ALL[i].as_bytes(), ALL[j].as_bytes()) {
                    out[i] |= 1 << j;
                }
                j += 1;
            }
            i += 1;
        }
        out
    };

    pub(crate) const WHITESPACE: TT = TT::named("Token.Text.Whitespace");
    pub(crate) const NEWLINE: TT = TT::named("Token.Text.Whitespace.Newline");
    pub(crate) const KEYWORD: TT = TT::named("Token.Keyword");
    pub(crate) const DML: TT = TT::named("Token.Keyword.DML");
    pub(crate) const DDL: TT = TT::named("Token.Keyword.DDL");
    pub(crate) const CTE: TT = TT::named("Token.Keyword.CTE");
    pub(crate) const ORDER: TT = TT::named("Token.Keyword.Order");
    pub(crate) const TZCAST: TT = TT::named("Token.Keyword.TZCast");
    pub(crate) const NAME: TT = TT::named("Token.Name");
    pub(crate) const NAME_BUILTIN: TT = TT::named("Token.Name.Builtin");
    pub(crate) const NAME_PLACEHOLDER: TT = TT::named("Token.Name.Placeholder");
    pub(crate) const STRING: TT = TT::named("Token.Literal.String");
    pub(crate) const STRING_SINGLE: TT = TT::named("Token.Literal.String.Single");
    pub(crate) const STRING_SYMBOL: TT = TT::named("Token.Literal.String.Symbol");
    pub(crate) const NUMBER: TT = TT::named("Token.Literal.Number");
    pub(crate) const NUMBER_INTEGER: TT = TT::named("Token.Literal.Number.Integer");
    pub(crate) const NUMBER_FLOAT: TT = TT::named("Token.Literal.Number.Float");
    pub(crate) const PUNCTUATION: TT = TT::named("Token.Punctuation");
    pub(crate) const OPERATOR: TT = TT::named("Token.Operator");
    pub(crate) const COMPARISON: TT = TT::named("Token.Operator.Comparison");
    pub(crate) const WILDCARD: TT = TT::named("Token.Wildcard");
    pub(crate) const COMMENT: TT = TT::named("Token.Comment");
    pub(crate) const MULTILINE_HINT: TT = TT::named("Token.Comment.Multiline.Hint");
    pub(crate) const SINGLE_HINT: TT = TT::named("Token.Comment.Single.Hint");
    pub(crate) const ASSIGNMENT: TT = TT::named("Token.Assignment");
}

#[derive(Clone, Copy)]
enum Val {
    /// A byte range of the statement's text.
    Span(u32, u32),
    /// One of the tree's own strings (`Tree::strs`).
    Owned(u32),
    /// `''`.
    Empty,
}

/// `Node::norm` when `normalized` is the value the node was made with.
const SAME: u32 = u32::MAX;

/// `Node::tid` of a group (no type).
const NONE: u8 = u8::MAX;
/// `Node::tid` of a type not in `tt::ALL`.
const OTHER: u8 = u8::MAX - 1;

/// `ttype`'s number in `tt::ALL`, or OTHER. Lexer types are static strings, so a small cache by address
/// answers nearly every call; a miss looks the name up and fills its slot.
fn type_id(ttype: &'static str) -> u8 {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CACHE: [AtomicUsize; 64] = [const { AtomicUsize::new(0) }; 64];
    let addr = ttype.as_ptr() as usize;
    let slot = &CACHE[(addr >> 3) & 63];
    let hit = slot.load(Ordering::Relaxed);
    if hit != 0
        && hit >> 8 == addr
        && ttype.len() == tt::ALL.get(hit & 0xff).map_or(usize::MAX, |n| n.len())
    {
        return (hit & 0xff) as u8;
    }
    match tt::ALL.iter().position(|n| *n == ttype) {
        Some(i) => {
            if addr >> 56 == 0 {
                slot.store(addr << 8 | i, Ordering::Relaxed);
            }
            i as u8
        }
        None => OTHER,
    }
}

pub(crate) struct Node {
    pub(crate) cls: Cls,
    pub(crate) ttype: Option<&'static str>,
    /// `ttype`'s number in `tt::ALL`, NONE for a group, OTHER for a type not in it (compared by name then).
    tid: u8,
    val: Val,
    /// `normalized` where it is not the value the node was made with: one of the tree's own strings, or SAME.
    norm: u32,
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
    /// Values and normalized values that are not slices of `text` (kept apart so that a node stays small).
    strs: Vec<String>,
    pub(crate) n: Vec<Node>,
    /// Copies of token lists that grouping passes walk (`snapshot`), one after another as the passes recurse.
    scratch: Vec<Id>,
    /// The types of the leaves made so far, one bit per number in `tt::ALL`; `other_types` if any type was not
    /// in it.
    types: u64,
    other_types: bool,
    /// Which of the words a grouping pass starts from (`w::*`) a leaf has had.
    words: u16,
    /// Bumped whenever a token list changes; `shape` is cached for one value of it.
    version: u64,
    shape: Option<(u64, usize, usize)>,
}

/// Words grouping passes start from: a leaf `Keyword` whose normalized value is that word, or a `Punctuation`
/// leaf of that value.
pub(crate) mod w {
    pub(crate) const CASE: u16 = 1;
    pub(crate) const IF: u16 = 1 << 1;
    pub(crate) const FOR: u16 = 1 << 2;
    pub(crate) const BEGIN: u16 = 1 << 3;
    pub(crate) const OVER: u16 = 1 << 4;
    pub(crate) const WHERE: u16 = 1 << 5;
    pub(crate) const TIMESTAMP: u16 = 1 << 6;
    pub(crate) const VALUES: u16 = 1 << 7;
    pub(crate) const BRACKET: u16 = 1 << 8;
    pub(crate) const PAREN: u16 = 1 << 9;
    pub(crate) const CAST: u16 = 1 << 10;

    pub(crate) fn keyword(normalized: &str) -> u16 {
        match normalized {
            "CASE" => CASE,
            "IF" => IF,
            "FOR" | "FOREACH" => FOR,
            "BEGIN" => BEGIN,
            "OVER" => OVER,
            "WHERE" => WHERE,
            "TIMESTAMP" => TIMESTAMP,
            "VALUES" => VALUES,
            _ => 0,
        }
    }

    pub(crate) fn punctuation(value: &str) -> u16 {
        match value {
            "[" => BRACKET,
            "(" => PAREN,
            "::" => CAST,
            _ => 0,
        }
    }
}

/// The statement, root of every tree.
pub(crate) const ROOT: Id = 0;

/// `a == b` for two token type names. They all start with `Token.`, so two of the same length almost always
/// differ in their last bytes: those are compared first, as one word, before the whole names.
#[inline]
fn same_type(a: &str, b: &str) -> bool {
    let (x, y) = (a.as_bytes(), b.as_bytes());
    if x.len() != y.len() {
        return false;
    }
    if std::ptr::eq(x.as_ptr(), y.as_ptr()) {
        return true;
    }
    if x.len() >= 8 {
        let tail = |s: &[u8]| u64::from_ne_bytes(s[s.len() - 8..].try_into().expect("8 bytes"));
        if tail(x) != tail(y) {
            return false;
        }
    }
    x == y
}

/// `value.upper()` for a keyword's `normalized`, None when it is the value itself.
fn keyword_norm(value: &str) -> Option<String> {
    if value.is_ascii() {
        // most keywords are already upper case: no new string for those
        value
            .bytes()
            .any(|b| b.is_ascii_lowercase())
            .then(|| value.to_ascii_uppercase())
    } else {
        let up = pystr::upper(value);
        (up != value).then_some(up)
    }
}

impl Tree {
    /// `sql.Statement([sql.Token(ttype, value), ...])` for one statement of the splitter.
    pub(crate) fn statement<S: AsRef<str>>(tokens: &[(&'static str, S)]) -> Tree {
        let size: usize = tokens.iter().map(|(_, v)| v.as_ref().len()).sum();
        let mut text = String::with_capacity(size);
        let mut tree = Tree {
            text: String::new(),
            strs: Vec::new(),
            n: Vec::with_capacity(tokens.len() * 2 + 1),
            scratch: Vec::new(),
            types: 0,
            other_types: false,
            words: 0,
            version: 0,
            shape: None,
        };
        tree.n
            .push(Self::group_node(Cls::Statement, Val::Span(0, size as u32)));
        let mut children = Vec::with_capacity(tokens.len());
        for (ttype, value) in tokens {
            let value = value.as_ref();
            let start = text.len() as u32;
            text.push_str(value);
            let id = tree.leaf(ttype, Val::Span(start, text.len() as u32), value);
            tree.n[id].parent = Some(ROOT);
            children.push(id);
        }
        tree.n[ROOT].tokens = children;
        tree.text = text;
        tree
    }

    fn own(&mut self, s: String) -> Val {
        if s.is_empty() {
            return Val::Empty;
        }
        self.strs.push(s);
        Val::Owned((self.strs.len() - 1) as u32)
    }

    /// A new leaf node with value `val` (whose text is `value`).
    fn leaf(&mut self, ttype: &'static str, val: Val, value: &str) -> Id {
        let tid = type_id(ttype);
        let is = |parent: TT| match tid {
            OTHER => is_a(ttype, parent.name),
            i => tt::ANCESTORS[i as usize] & (1 << parent.id) != 0,
        };
        let is_keyword = is(tt::KEYWORD);
        let norm = match is_keyword.then(|| keyword_norm(value)).flatten() {
            Some(up) => {
                self.strs.push(up);
                (self.strs.len() - 1) as u32
            }
            None => SAME,
        };
        let (is_whitespace, is_newline) = (is(tt::WHITESPACE), is(tt::NEWLINE));
        match tid {
            OTHER => self.other_types = true,
            i => self.types |= 1 << i,
        }
        if tid == tt::KEYWORD.id {
            self.words |= w::keyword(match norm {
                SAME => value,
                i => &self.strs[i as usize],
            });
        } else if tid == tt::PUNCTUATION.id {
            self.words |= w::punctuation(value);
        }
        self.n.push(Node {
            cls: Cls::Token,
            ttype: Some(ttype),
            tid,
            val,
            norm,
            changed: false,
            is_keyword,
            is_whitespace,
            is_newline,
            parent: None,
            tokens: Vec::new(),
        });
        self.n.len() - 1
    }

    fn group_node(cls: Cls, val: Val) -> Node {
        Node {
            cls,
            ttype: None,
            tid: NONE,
            val,
            norm: SAME,
            changed: false,
            is_keyword: false,
            is_whitespace: false,
            is_newline: false,
            parent: None,
            tokens: Vec::new(),
        }
    }

    /// A new `sql.Token(ttype, value)` (no parent yet).
    pub(crate) fn new_token(&mut self, ttype: TT, value: String) -> Id {
        let id = self.leaf(ttype.name, Val::Empty, &value);
        self.n[id].val = self.own(value);
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
    fn joined(&mut self, tokens: &[Id]) -> Val {
        let mut span: Option<(u32, u32)> = None;
        let mut adjacent = true;
        for &c in tokens {
            match (self.n[c].val, span) {
                (Val::Span(a, b), None) => span = Some((a, b)),
                (Val::Span(a, b), Some((s, e))) if a == e => span = Some((s, b)),
                (Val::Span(a, b), Some(_)) if a == b => {}
                (Val::Empty, _) => {}
                _ => {
                    adjacent = false;
                    break;
                }
            }
        }
        match (adjacent, span) {
            (true, Some((s, e))) => Val::Span(s, e),
            (true, None) => Val::Empty,
            (false, _) => {
                let s: String = tokens.iter().map(|&c| self.value(c)).collect();
                self.own(s)
            }
        }
    }

    fn val_str(&self, val: Val) -> &str {
        match val {
            Val::Span(a, b) => &self.text[a as usize..b as usize],
            Val::Owned(i) => &self.strs[i as usize],
            Val::Empty => "",
        }
    }

    pub(crate) fn value(&self, id: Id) -> &str {
        self.val_str(self.n[id].val)
    }

    fn keep_normalized(&mut self, id: Id) {
        if !self.n[id].changed {
            if self.n[id].norm == SAME {
                let v = self.value(id).to_string();
                self.strs.push(v);
                self.n[id].norm = (self.strs.len() - 1) as u32;
            }
            self.n[id].changed = true;
        }
    }

    /// `token.value = value`.
    pub(crate) fn set_value(&mut self, id: Id, value: String) {
        self.keep_normalized(id);
        self.n[id].val = self.own(value);
    }

    /// `grp.value += ''.join(token.value for token in tokens)`.
    fn append_value(&mut self, id: Id, tokens: &[Id]) {
        let more = self.joined(tokens);
        let merged = match (self.n[id].val, more) {
            (_, Val::Empty) => return,
            (Val::Span(a, b), Val::Span(c, d)) if b == c => Val::Span(a, d),
            (cur, more) => {
                let mut s = self.val_str(cur).to_string();
                s.push_str(self.val_str(more));
                self.own(s)
            }
        };
        self.keep_normalized(id);
        self.n[id].val = merged;
    }

    /// A copy of `l`'s token list as it is now, for a pass that walks it while changing it: `(base, len)`, the
    /// copy being `snapshot_at(base + i)` for `i < len`. Copies are stacked in one buffer; `release(base)`
    /// drops this one and any taken after it.
    pub(crate) fn snapshot(&mut self, l: Id) -> (usize, usize) {
        let base = self.scratch.len();
        let Tree { scratch, n, .. } = self;
        scratch.extend_from_slice(&n[l].tokens);
        (base, scratch.len() - base)
    }

    pub(crate) fn snapshot_at(&self, i: usize) -> Id {
        self.scratch[i]
    }

    pub(crate) fn release(&mut self, base: usize) {
        self.scratch.truncate(base);
    }

    pub(crate) fn normalized(&self, id: Id) -> &str {
        match self.n[id].norm {
            SAME => self.value(id),
            i => &self.strs[i as usize],
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

    /// `token.ttype = t` (group_operator retypes a token).
    pub(crate) fn set_ttype(&mut self, id: Id, t: TT) {
        self.n[id].ttype = Some(t.name);
        self.n[id].tid = t.id;
        self.types |= 1 << t.id;
    }

    /// Whether a leaf of exactly type `t` may be in the tree (false only when there surely is none).
    pub(crate) fn may_have_type(&self, t: TT) -> bool {
        self.other_types || self.types & (1 << t.id) != 0
    }

    /// Whether a leaf of type `parent` or a subtype of it may be in the tree.
    pub(crate) fn may_have_type_in(&self, parent: TT) -> bool {
        self.other_types
            || (0..tt::ALL.len())
                .any(|i| self.types & (1 << i) != 0 && tt::ANCESTORS[i] & (1 << parent.id) != 0)
    }

    /// Whether a leaf has had one of `words` (`w::*`).
    pub(crate) fn may_have_word(&self, words: u16) -> bool {
        self.words & words != 0
    }

    /// The tree's height (levels of groups below the statement) and its longest token list.
    pub(crate) fn shape(&mut self) -> (usize, usize) {
        if let Some((v, h, len)) = self.shape
            && v == self.version
        {
            return (h, len);
        }
        let (mut height, mut longest) = (0, 0);
        let mut stack = vec![(ROOT, 0usize)];
        while let Some((g, level)) = stack.pop() {
            height = height.max(level);
            longest = longest.max(self.n[g].tokens.len());
            for &c in &self.n[g].tokens {
                if self.n[c].cls != Cls::Token {
                    stack.push((c, level + 1));
                }
            }
        }
        self.shape = Some((self.version, height, longest));
        (height, longest)
    }

    /// `token.ttype is t` (and `token.ttype == t`).
    #[inline]
    pub(crate) fn ttype_is(&self, id: Id, t: TT) -> bool {
        let n = &self.n[id];
        match n.tid {
            NONE => false,
            OTHER => n.ttype.is_some_and(|x| same_type(x, t.name)),
            i => i == t.id,
        }
    }

    /// `token.ttype in (a, b, ...)` with a plain tuple: equality.
    pub(crate) fn ttype_in(&self, id: Id, ts: &[TT]) -> bool {
        ts.iter().any(|&t| self.ttype_is(id, t))
    }

    /// `token.ttype in T.X`: X or a subtype of it.
    #[inline]
    pub(crate) fn ttype_is_a(&self, id: Id, parent: TT) -> bool {
        let n = &self.n[id];
        match n.tid {
            NONE => false,
            OTHER => n.ttype.is_some_and(|t| is_a(t, parent.name)),
            i => tt::ANCESTORS[i as usize] & (1 << parent.id) != 0,
        }
    }

    /// `token.match(ttype, values)` with a list of values (`values=None` is `ttype_is`).
    pub(crate) fn matches(&self, id: Id, ttype: TT, values: &[&str]) -> bool {
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
        self.version += 1;
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
        self.version += 1;
        self.n[token].parent = Some(l);
        let idx = idx.min(self.n[l].tokens.len());
        self.n[l].tokens.insert(idx, token);
    }

    /// `tlist.insert_after(where, token, skip_ws)` with an index.
    pub(crate) fn insert_after(&mut self, l: Id, idx: usize, token: Id, skip_ws: bool) {
        self.version += 1;
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
