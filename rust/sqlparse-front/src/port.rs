//! The Rust port of `front/sqlparse_front.py`.
//!
//! Replace each `unimplemented!` with the port of the source function. Return `Err` where
//! the source throws or raises: the caller gets an error there too. After changing a
//! signature, run `rust-anything refresh` to regenerate the bindings. `rust-anything diff` checks
//! behavior against what the source recorded.

#![forbid(unsafe_code)]

#[path = "lexer.rs"]
mod lexer;
#[path = "lexer_tables.rs"]
#[rustfmt::skip]
mod lexer_tables;
#[path = "filters.rs"]
mod filters;
#[path = "format.rs"]
mod format;
#[path = "grouping.rs"]
mod grouping;
#[path = "pystr.rs"]
mod pystr;
#[path = "split.rs"]
mod split;
#[path = "tree.rs"]
mod tree;
#[path = "pyunicode_tables.rs"]
#[rustfmt::skip]
mod pyunicode_tables;

/// Not part of the front door: `tokenize` as (token type, text) pairs, for the package's lexer.
pub fn lex(text: &str) -> Vec<(&'static str, &str)> {
    lexer::tokenize(text)
}

/// Not part of the front door: every token type `lex` can give.
pub fn token_type_names() -> Vec<&'static str> {
    lexer::type_names()
}

/// Not part of the front door: `sqlparse.parse(sql)`'s statements, grouped in Rust, as a tree per statement.
/// The recursion limit is counted as for `sqlparse.parse` called where the package's `parse` stands.
pub struct Parsed(Vec<tree::Tree>);

/// See [`Parsed`]. With `run`, the recursion limit is counted as for sqlparse's `FilterStack.run` generator,
/// whose frame the caller's stands in for (the package's replacement of it): no `parse` frame above it, and on
/// up to 3.11 no `tuple()` C call either.
pub fn parse_trees(sql: &str, run: bool) -> Result<Parsed, String> {
    let stack = if run {
        python_stack_from(PARSE_ENTRY_FRAMES_311, RUN_FRAMES_311, RUN_FRAMES_312)
    } else {
        python_stack(PARSE_ENTRY_FRAMES_311)
    };
    format::parse(sql, &stack).map(Parsed)
}

/// Not part of the front door: `FilterStack.run`'s statements one at a time, as sqlparse's run gives them: each
/// statement is lexed, split off and grouped when it is asked for, with the recursion limit counted from where
/// the caller then stands. Only the text and one statement's tokens are held, as sqlparse's generators hold them.
pub struct Lazy {
    text: String,
    spans: lexer::Spans,
    /// Where the lexer goes on from.
    pos: usize,
    splitter: split::Feed,
    /// The first token of the next statement as (type, start, end) in `text`: read, and given to the splitter,
    /// when it ended the statement before it.
    pending: Option<(&'static str, usize, usize)>,
    /// The statement `next` or `skip` last took, as (type, start, end) in `text`.
    current: Vec<(&'static str, usize, usize)>,
}

/// A caller's own `AlignedIndentFilter` as found: `n`, `char`, `offset`, `indent`, `_max_kwd_len`.
pub type AlignedState = (String, String, i64, i64, i64);

/// A caller's own `ReindentFilter` as found: `n`, `char`, `width`, `indent`, `offset`, `wrap_after`,
/// `comma_first`, `indent_columns`, `compact`, `len(_last_func.value)` (None while `_last_func` is None) and
/// `str(_last_stmt).endswith('\n')` (None while `_last_stmt` is None).
pub type ReindentState = (
    String,
    String,
    i64,
    i64,
    i64,
    i64,
    bool,
    bool,
    bool,
    Option<i64>,
    Option<bool>,
);

/// One statement from [`Lazy`]: the tree; whether the caller's first statement filter was run on it here; for
/// an `AlignedIndentFilter`, the `TokenList` that filter wraps the statement's tokens in; for a
/// `ReindentFilter`, the node its `_last_func` was set to, if it was.
pub struct Grouped {
    tree: tree::Tree,
    filtered: bool,
    temp: Option<filters::TempList>,
    last_func: Option<tree::Id>,
}

impl Grouped {
    fn plain(tree: tree::Tree) -> Grouped {
        Grouped {
            tree,
            filtered: false,
            temp: None,
            last_func: None,
        }
    }
}

/// Whether `id` is in the statement, through the token lists from the root.
fn reaches(t: &tree::Tree, id: tree::Id) -> bool {
    let mut todo = vec![tree::ROOT];
    while let Some(n) = todo.pop() {
        if n == id {
            return true;
        }
        todo.extend_from_slice(t.tokens(n));
    }
    false
}

/// Up to what height (and frames left to Python's limit) the Rust runs a caller's `AlignedIndentFilter`: the
/// filter takes a few Python frames a level (`_process`, `_process_default`, `_process_parenthesis`) and a few
/// more for a look (`_next_token`, `token_next_by`, `match`, `str()` down a group). Past these the statement goes
/// to the Python filter, which then raises what it raises, where it raises it.
const ALIGNED_HEIGHT: usize = 60;
const ALIGNED_FRAMES_PER_LEVEL: usize = 4;
const ALIGNED_FRAMES: usize = 40;

/// The same for a caller's `ReindentFilter`: `_process` and `_process_default` a level, `str()` down a group
/// before a keyword, and `_next_token`, `token_next_by` and the matching for a look.
const REINDENT_HEIGHT: usize = 60;
const REINDENT_FRAMES_PER_LEVEL: usize = 4;
const REINDENT_FRAMES: usize = 40;

impl Lazy {
    pub fn new(text: String) -> Lazy {
        // sqlparse's lexer finds the delimited spans (dollar quotes, block comments) over the whole text first
        let spans = lexer::find_delimited_spans(&text);
        Lazy {
            text,
            spans,
            pos: 0,
            splitter: split::Feed::default(),
            pending: None,
            current: Vec::new(),
        }
    }

    /// Lexes the next statement into `current`, as sqlparse's splitter cuts it; false after the last.
    fn take(&mut self) -> bool {
        self.current.clear();
        self.current.extend(self.pending.take());
        while let Some((ttype, end)) = lexer::token_at(&self.text, &self.spans, self.pos) {
            let start = std::mem::replace(&mut self.pos, end);
            if self.splitter.token(ttype, &self.text[start..end]) {
                self.pending = Some((ttype, start, end));
                return true;
            }
            self.current.push((ttype, start, end));
        }
        split::is_statement(self.current.iter().map(|&(ttype, _, _)| ttype))
    }

    /// The next statement grouped, then the caller's first statement filter (an `AlignedIndentFilter` or a
    /// `ReindentFilter`, whose state is given) run on it when the statement is well within what the Python filter
    /// does without hitting a limit; None after the last.
    pub fn next(
        &mut self,
        aligned: Option<AlignedState>,
        reindent: Option<ReindentState>,
    ) -> Result<Option<Grouped>, String> {
        if !self.take() {
            return Ok(None);
        }
        let stack = python_stack_from(PARSE_ENTRY_FRAMES_311, RUN_FRAMES_311, RUN_FRAMES_312);
        let view: Vec<(&'static str, &str)> = self
            .current
            .iter()
            .map(|&(ttype, a, b)| (ttype, &self.text[a..b]))
            .collect();
        let mut tree = tree::Tree::statement(&view);
        grouping::group(&mut tree, &stack, 3)?;
        filters::reset_indent_budget();
        // the statement as grouping left it, for the Python filter to run on (and raise what it raises)
        let regrouped = || -> Result<Option<Grouped>, String> {
            let mut tree = tree::Tree::statement(&view);
            grouping::group(&mut tree, &stack, 3)?;
            Ok(Some(Grouped::plain(tree)))
        };
        if let Some((n, ch, offset, indent, max_kwd_len)) = aligned {
            let (height, _) = tree.shape();
            if height > ALIGNED_HEIGHT
                || stack.may_overflow(ALIGNED_FRAMES_PER_LEVEL * height + ALIGNED_FRAMES)
            {
                return Ok(Some(Grouped::plain(tree)));
            }
            let mut f = filters::Aligned::with_state(
                n,
                ch,
                offset as i128,
                indent as i128,
                max_kwd_len as i128,
                &stack,
            );
            return match f.run(&mut tree) {
                Ok(()) => Ok(Some(Grouped {
                    tree,
                    filtered: true,
                    temp: f.temp.take(),
                    last_func: None,
                })),
                Err(_) => regrouped(),
            };
        }
        let Some(state) = reindent else {
            return Ok(Some(Grouped::plain(tree)));
        };
        let (height, _) = tree.shape();
        if height > REINDENT_HEIGHT
            || stack.may_overflow(REINDENT_FRAMES_PER_LEVEL * height + REINDENT_FRAMES)
        {
            return Ok(Some(Grouped::plain(tree)));
        }
        let (n, ch, width, indent, offset, wrap_after, comma_first, indent_columns, compact) = (
            state.0, state.1, state.2, state.3, state.4, state.5, state.6, state.7, state.8,
        );
        let mut f = filters::Reindent::with_state(
            filters::ReindentFound {
                n,
                ch,
                width,
                indent,
                offset,
                wrap_after,
                comma_first,
                indent_columns,
                compact,
                last_func_len: state.9,
                last_stmt_nl: state.10,
            },
            &stack,
        );
        match f.run(&mut tree) {
            // the token _last_func is left at must be one the statement's objects include
            Ok(()) if f.last_func.is_none_or(|id| reaches(&tree, id)) => Ok(Some(Grouped {
                last_func: f.last_func,
                tree,
                filtered: true,
                temp: None,
            })),
            _ => regrouped(),
        }
    }
}

impl Lazy {
    /// Takes the next statement without grouping it (false after the last): `last_tokens` then gives it.
    pub fn skip(&mut self) -> bool {
        self.take()
    }

    /// The tokens of the statement `next` or `skip` last took, as (type name, text): for the package to make
    /// that statement as sqlparse's splitter does when it is handed to sqlparse.
    pub fn last_tokens(&self) -> Vec<(&'static str, &str)> {
        self.current
            .iter()
            .map(|&(ttype, a, b)| (ttype, &self.text[a..b]))
            .collect()
    }
}

impl Grouped {
    /// Whether the caller's first statement filter was run on the statement here.
    pub fn filtered(&self) -> bool {
        self.filtered
    }

    /// The statement as sqlparse's own objects (see [`Parsed::to_python`]), and the object the caller's
    /// `ReindentFilter._last_func` was set to here, if it was.
    pub fn to_python<'py>(
        &self,
        py: pyo3::Python<'py>,
        sql: &SqlObjects,
    ) -> pyo3::PyResult<StatementObjects<'py>> {
        tree_object(py, &self.tree, sql, self.temp.as_ref(), self.last_func)
    }
}

/// A statement's object, and the object made for one of its nodes when asked for.
pub type StatementObjects<'py> = (
    pyo3::Bound<'py, pyo3::PyAny>,
    Option<pyo3::Bound<'py, pyo3::PyAny>>,
);

/// What `sqlparse.parse` has on the stack above `FilterStack.run`'s frame that a call from `run` itself has not,
/// by version (the `tuple()` C call counts up to 3.11 only). Measured with tools/check_recursion.py package.
const RUN_FRAMES_311: usize = 1;
const RUN_FRAMES_312: usize = 1;

/// What `Parsed::to_python` needs from `sqlparse`: its `sql` classes by name, its token types by `str()`,
/// and `object.__new__`.
pub struct SqlObjects {
    pub classes: std::collections::HashMap<String, pyo3::Py<pyo3::PyAny>>,
    pub types: std::collections::HashMap<String, pyo3::Py<pyo3::PyAny>>,
    pub new: pyo3::Py<pyo3::PyAny>,
    /// How to make and fill the objects without Python calls, when that was found to work (see `slots`).
    pub layout: Option<crate::slots::Layout>,
}

impl Parsed {
    /// The statements as sqlparse's own objects, as `sqlparse.parse` leaves them: each node made with
    /// `object.__new__` and its slots set from the tree (`__init__` would recompute what grouping changed
    /// since: a group's value joined at creation, `normalized` taken at creation, a token's parent).
    pub fn to_python<'py>(
        &self,
        py: pyo3::Python<'py>,
        sql: &SqlObjects,
    ) -> pyo3::PyResult<Vec<pyo3::Bound<'py, pyo3::PyAny>>> {
        self.0
            .iter()
            .map(|t| tree_object(py, t, sql, None, None).map(|(root, _)| root))
            .collect()
    }
}

fn tree_object<'py>(
    py: pyo3::Python<'py>,
    t: &tree::Tree,
    sql: &SqlObjects,
    temp: Option<&filters::TempList>,
    want: Option<tree::Id>,
) -> pyo3::PyResult<StatementObjects<'py>> {
    use pyo3::prelude::*;
    use pyo3::types::{PyList, PyString};
    use pyo3::{IntoPyObjectExt, intern};
    let missing = |what: &str| pyo3::exceptions::PyKeyError::new_err(what.to_string());
    // every node the statement reaches, through its tokens or a parent pointer, gets its object first
    let mut objs: Vec<Option<Bound<'py, PyAny>>> = vec![None; t.n.len()];
    let mut order = Vec::new();
    let mut todo = vec![tree::ROOT];
    while let Some(id) = todo.pop() {
        if objs[id].is_some() {
            continue;
        }
        let name = t.cls(id).name();
        let cls = sql.classes.get(name).ok_or_else(|| missing(name))?;
        objs[id] = Some(match &sql.layout {
            Some(layout) => layout.make(cls.bind(py))?,
            None => sql.new.bind(py).call1((cls.bind(py),))?,
        });
        order.push(id);
        todo.extend(t.tokens(id).iter().rev());
        if let Some(p) = t.n[id].parent {
            todo.push(p);
        }
    }
    // the lexer's and grouping's type names are a handful of static strings: by address first
    let mut seen: Vec<(usize, Bound<'py, PyAny>)> = Vec::with_capacity(32);
    let none = py.None().into_bound(py);
    for &id in &order {
        let node = &t.n[id];
        let obj = objs[id].as_ref().expect("made above");
        let value = t.value(id);
        let py_value = PyString::new(py, value);
        let ttype = match node.ttype {
            None => none.clone(),
            Some(tt) => {
                let addr = tt.as_ptr() as usize;
                match seen.iter().find(|(a, _)| *a == addr) {
                    Some((_, o)) => o.clone(),
                    None => {
                        let o = sql
                            .types
                            .get(tt)
                            .ok_or_else(|| missing(tt))?
                            .bind(py)
                            .clone();
                        seen.push((addr, o.clone()));
                        o
                    }
                }
            }
        };
        let normalized = t.normalized(id);
        let py_normalized = if normalized == value {
            py_value.clone()
        } else {
            PyString::new(py, normalized)
        };
        let parent = match node.parent {
            Some(p) => objs[p].clone().expect("reached above"),
            None => none.clone(),
        };
        if let Some(layout) = &sql.layout {
            let tokens = if t.is_group(id) {
                let children = t
                    .tokens(id)
                    .iter()
                    .map(|&c| objs[c].clone().expect("reached above"));
                Some(PyList::new(py, children)?)
            } else {
                None
            };
            layout.fill(
                obj,
                crate::slots::Slots {
                    value: &py_value,
                    ttype: &ttype,
                    parent: &parent,
                    is_group: t.is_group(id),
                    is_keyword: node.is_keyword,
                    is_whitespace: node.is_whitespace,
                    is_newline: node.is_newline,
                    normalized: &py_normalized,
                    tokens,
                },
            );
            continue;
        }
        obj.setattr(intern!(py, "value"), py_value)?;
        obj.setattr(intern!(py, "ttype"), ttype)?;
        obj.setattr(intern!(py, "parent"), parent)?;
        obj.setattr(intern!(py, "is_group"), t.is_group(id))?;
        obj.setattr(intern!(py, "is_keyword"), node.is_keyword)?;
        obj.setattr(intern!(py, "is_whitespace"), node.is_whitespace)?;
        obj.setattr(intern!(py, "is_newline"), node.is_newline)?;
        obj.setattr(intern!(py, "normalized"), py_normalized)?;
        if t.is_group(id) {
            let children = t
                .tokens(id)
                .iter()
                .map(|&c| objs[c].clone().expect("reached above"));
            obj.setattr(intern!(py, "tokens"), PyList::new(py, children)?)?;
        }
    }
    let root = objs[tree::ROOT].clone().expect("the root");
    if let Some(temp) = temp {
        // sql.TokenList(stmt.tokens): Token.__init__(None, value), then is_group; the statement's children (and
        // what was inserted at the top since) have it as their parent
        let name = "TokenList";
        let cls = sql.classes.get(name).ok_or_else(|| missing(name))?;
        let obj = sql.new.bind(py).call1((cls.bind(py),))?;
        let value = PyString::new(py, &temp.value);
        obj.setattr(intern!(py, "value"), &value)?;
        obj.setattr(intern!(py, "ttype"), &none)?;
        obj.setattr(intern!(py, "parent"), &none)?;
        obj.setattr(intern!(py, "is_group"), true)?;
        obj.setattr(intern!(py, "is_keyword"), false)?;
        obj.setattr(intern!(py, "is_whitespace"), false)?;
        obj.setattr(intern!(py, "is_newline"), false)?;
        obj.setattr(intern!(py, "normalized"), &value)?;
        let tokens = if temp.shares_tokens {
            root.getattr(intern!(py, "tokens"))?
        } else {
            PyList::empty(py).into_any()
        };
        obj.setattr(intern!(py, "tokens"), tokens)?;
        for &c in t.tokens(tree::ROOT) {
            objs[c]
                .as_ref()
                .expect("reached above")
                .setattr(intern!(py, "parent"), &obj)?;
        }
    }
    let wanted = want.and_then(|id| objs.get(id).cloned().flatten());
    Ok((root.into_bound_py_any(py)?, wanted))
}

/// One bit per code point: those the running Python's Unicode tables treat differently from the CPython 3.11
/// tables the port was generated from (`python/sqlparse_turbo/_unicode_drift.py`). Set once by the package.
static DRIFT: std::sync::OnceLock<Vec<u64>> = std::sync::OnceLock::new();

/// Not part of the front door: the package hands over its drift list once, at import. Later calls are ignored
/// (one process runs one Python, so one Unicode version).
pub fn set_drift(ranges: &[(u32, u32)]) {
    let mut bits = vec![0u64; 0x11_0000usize.div_ceil(64)];
    for &(a, b) in ranges {
        for cp in a..=b.min(0x10_FFFF) {
            bits[cp as usize / 64] |= 1 << (cp % 64);
        }
    }
    let _ = DRIFT.set(bits);
}

/// Whether `text` holds a code point of the drift list (false before `set_drift`).
pub fn drifted(text: &str) -> bool {
    let Some(bits) = DRIFT.get() else {
        return false;
    };
    !text.is_ascii()
        && text.chars().any(|c| {
            let cp = c as usize;
            cp >= 0x80 && bits[cp / 64] >> (cp % 64) & 1 == 1
        })
}

/// Port of `tokenize` (`front/sqlparse_front.py:13`).
pub fn tokenize(sql: String) -> Result<Vec<serde_json::Value>, String> {
    Ok(lexer::tokenize(&sql)
        .into_iter()
        .map(|(ttype, value)| serde_json::json!({"$tuple": [ttype, value]}))
        .collect())
}

/// Port of `split` (`front/sqlparse_front.py:18`).
///
/// `strip_semicolon` defaults to `False` in the source.
pub fn split(sql: String, strip_semicolon: bool) -> Result<Vec<String>, String> {
    Ok(split::split(&sql, strip_semicolon))
}

/// Port of `format` (`front/sqlparse_front.py:23`).
///
/// `keyword_case` defaults to `None` in the source.
///
/// `identifier_case` defaults to `None` in the source.
///
/// `strip_comments` defaults to `False` in the source.
///
/// `use_space_around_operators` defaults to `False` in the source.
///
/// `strip_whitespace` defaults to `False` in the source.
///
/// `truncate_strings` defaults to `None` in the source.
///
/// `truncate_char` defaults to `'[...]'` in the source.
///
/// `reindent` defaults to `False` in the source.
///
/// `reindent_aligned` defaults to `False` in the source.
///
/// `indent_columns` defaults to `False` in the source.
///
/// `indent_after_first` defaults to `False` in the source.
///
/// `indent_tabs` defaults to `False` in the source.
///
/// `indent_width` defaults to `2` in the source.
///
/// `wrap_after` defaults to `0` in the source.
///
/// `comma_first` defaults to `False` in the source.
///
/// `compact` defaults to `False` in the source.
///
/// `right_margin` defaults to `None` in the source.
///
/// `output_format` defaults to `None` in the source.
pub fn format(
    sql: String,
    keyword_case: Option<String>,
    identifier_case: Option<String>,
    strip_comments: bool,
    use_space_around_operators: bool,
    strip_whitespace: bool,
    truncate_strings: Option<i64>,
    truncate_char: String,
    reindent: bool,
    reindent_aligned: bool,
    indent_columns: bool,
    indent_after_first: bool,
    indent_tabs: bool,
    indent_width: i64,
    wrap_after: i64,
    comma_first: bool,
    compact: bool,
    right_margin: Option<i64>,
    output_format: Option<String>,
) -> Result<String, String> {
    format::format(
        &sql,
        &format::Options {
            keyword_case,
            identifier_case,
            strip_comments,
            use_space_around_operators,
            strip_whitespace,
            truncate_strings,
            truncate_char,
            reindent,
            reindent_aligned,
            indent_columns,
            indent_after_first,
            indent_tabs,
            indent_width,
            wrap_after,
            comma_first,
            compact,
            right_margin,
            output_format,
        },
        &python_stack(FORMAT_ENTRY_FRAMES_311),
    )
}

/// The Python package's module (`package/python/sqlparse_turbo`). Its `format` stands where `sqlparse.format`
/// stands, so a call from it has one frame on the stack that sqlparse counts in the entry point.
const PACKAGE: &str = "sqlparse_turbo";

/// Frames on sqlparse's way in that CPython up to 3.11 counts against the recursion limit and 3.12 and later
/// do not (3.12 stopped counting C calls such as `''.join(...)` and `tuple(...)`, and inlines comprehensions):
/// `format` has two, `parse` one. The exact probe, run from the Rust, sees one of them as well on 3.12 (the call
/// into the bindings no longer counts either), so the shift is one less. Measured with tools/check_recursion.py
/// on CPython 3.10-3.14.
const FORMAT_ENTRY_FRAMES_311: usize = 2;
const PARSE_ENTRY_FRAMES_311: usize = 1;

/// Where Python's recursion limit stands for this call: the frames on the stack, and
/// `sys.getrecursionlimit()`. Called from `front/sqlparse_front.py`'s place, the front door's frame is
/// added (the original has it, the bindings do not). Called from the package, whose frame replaces
/// `sqlparse.format`'s (already counted in the entry point), there is neither, so two frames come off.
/// `entry_frames_311` is FORMAT_ENTRY_FRAMES_311 or PARSE_ENTRY_FRAMES_311, for the entry point being run.
fn python_stack(entry_frames_311: usize) -> grouping::Stack {
    python_stack_from(entry_frames_311, 0, 0)
}

/// [`python_stack`], with `fewer_311` / `fewer_312` frames less above the caller than the entry point has (up to
/// 3.11 / from 3.12).
fn python_stack_from(
    entry_frames_311: usize,
    fewer_311: usize,
    fewer_312: usize,
) -> grouping::Stack {
    use pyo3::prelude::*;
    pyo3::Python::attach(|py| -> Option<grouping::Stack> {
        let sys = py.import("sys").ok()?;
        let limit: usize = sys.call_method0("getrecursionlimit").ok()?.extract().ok()?;
        let mut frame = sys.call_method1("_getframe", (0,)).ok()?;
        let caller: Option<String> = frame
            .getattr("f_globals")
            .ok()?
            .get_item("__name__")
            .ok()
            .and_then(|n| n.extract().ok());
        let mut shift = if caller.as_deref() == Some(PACKAGE) {
            2
        } else {
            0
        };
        let minor: usize = sys
            .getattr("version_info")
            .ok()?
            .getattr("minor")
            .ok()?
            .extract()
            .ok()?;
        if minor >= 12 {
            shift += entry_frames_311 - 1 + fewer_312;
        } else {
            shift += fewer_311;
        }
        let mut depth = 1;
        loop {
            let back = frame.getattr("f_back").ok()?;
            if back.is_none() {
                break;
            }
            depth += 1;
            frame = back;
        }
        Some(grouping::Stack::python(depth + 1, shift, limit, exact_base))
    })
    .unwrap_or_else(grouping::Stack::exhausted)
}

/// What the probe's count is off by, against counting frames (measured with tools/check_recursion.py).
const PROBE_OFFSET: usize = 1;

/// The exact `base` for a front-door call made from here: how many more Python calls fit before
/// RecursionError, taken from the limit.
fn exact_base() -> Option<usize> {
    use pyo3::prelude::*;
    pyo3::Python::attach(|py| -> Option<usize> {
        let code = c"def probe():\n    n = 0\n    def down():\n        nonlocal n\n        n += 1\n        down()\n    try:\n        down()\n    except RecursionError:\n        pass\n    return n\n";
        let scope = pyo3::types::PyDict::new(py);
        py.run(code, Some(&scope), None).ok()?;
        let fits: usize = scope
            .get_item("probe")
            .ok()??
            .call0()
            .ok()?
            .extract()
            .ok()?;
        let limit: usize = py
            .import("sys")
            .ok()?
            .call_method0("getrecursionlimit")
            .ok()?
            .extract()
            .ok()?;
        limit.checked_sub(fits + PROBE_OFFSET)
    })
}

fn tree_value(t: &tree::Tree, id: tree::Id, parent: Option<tree::Id>) -> serde_json::Value {
    let mark = if t.n[id].parent == parent {
        ""
    } else {
        "!parent"
    };
    if t.is_group(id) {
        let children: Vec<serde_json::Value> = t
            .tokens(id)
            .iter()
            .map(|&c| tree_value(t, c, Some(id)))
            .collect();
        serde_json::json!([format!("{}{mark}", t.cls(id).name()), t.value(id), children])
    } else {
        let ttype = t.n[id].ttype.unwrap_or("None");
        serde_json::json!([format!("{ttype}{mark}"), t.value(id), t.normalized(id)])
    }
}

/// Port of `parse_tree` (`front/sqlparse_front.py`).
pub fn parse_tree(sql: String) -> Result<Vec<serde_json::Value>, String> {
    let trees = format::parse(&sql, &python_stack(PARSE_ENTRY_FRAMES_311))?;
    Ok(trees
        .iter()
        .map(|t| tree_value(t, tree::ROOT, None))
        .collect())
}
