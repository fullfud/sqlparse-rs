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
        self.0.iter().map(|t| tree_object(py, t, sql)).collect()
    }
}

fn tree_object<'py>(
    py: pyo3::Python<'py>,
    t: &tree::Tree,
    sql: &SqlObjects,
) -> pyo3::PyResult<pyo3::Bound<'py, pyo3::PyAny>> {
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
        objs[id] = Some(sql.new.bind(py).call1((cls.bind(py),))?);
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
    objs[tree::ROOT]
        .clone()
        .expect("the root")
        .into_bound_py_any(py)
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
    .unwrap_or_else(grouping::Stack::unlimited)
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
