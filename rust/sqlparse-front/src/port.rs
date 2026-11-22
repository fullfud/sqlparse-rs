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
        &python_stack(),
    )
}

/// The Python package's module (`package/python/sqlparse_rs`). Its `format` stands where `sqlparse.format`
/// stands, so a call from it has one frame on the stack that sqlparse counts in the entry point.
const PACKAGE: &str = "sqlparse_rs";

/// Where Python's recursion limit stands for this call: the frames on the stack, and
/// `sys.getrecursionlimit()`. Called from `front/sqlparse_front.py`'s place, the front door's frame is
/// added (the original has it, the bindings do not). Called from the package, whose frame replaces
/// `sqlparse.format`'s (already counted in the entry point), there is neither, so two frames come off.
fn python_stack() -> grouping::Stack {
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
        let shift = if caller.as_deref() == Some(PACKAGE) {
            2
        } else {
            0
        };
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
    let trees = format::parse(&sql, &python_stack())?;
    Ok(trees
        .iter()
        .map(|t| tree_value(t, tree::ROOT, None))
        .collect())
}
