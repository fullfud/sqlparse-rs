//! Python bindings for `front/sqlparse_front.py`: generated from the signatures in `port.rs` by rust-anything.
//! Regenerate with `rust-anything refresh` after changing them.

#![allow(clippy::too_many_arguments)]

use pyo3::prelude::*;

pub mod js;
pub mod port;
pub mod pyconv;
pub mod pynum;

#[allow(unused_imports)]
use pynum::Num;

fn error(message: String) -> PyErr {
    use pyo3::exceptions as e;
    let (kind, text) = message.split_once(": ").unwrap_or(("", message.as_str()));
    let text = text.to_string();
    match kind {
        "TypeError" => e::PyTypeError::new_err(text),
        "KeyError" => e::PyKeyError::new_err(text),
        "IndexError" => e::PyIndexError::new_err(text),
        "OverflowError" => e::PyOverflowError::new_err(text),
        "ZeroDivisionError" => e::PyZeroDivisionError::new_err(text),
        "AttributeError" => e::PyAttributeError::new_err(text),
        "RuntimeError" => e::PyRuntimeError::new_err(text),
        "NotImplementedError" => e::PyNotImplementedError::new_err(text),
        "LookupError" => e::PyLookupError::new_err(text),
        "ArithmeticError" => e::PyArithmeticError::new_err(text),
        "UnicodeError" => e::PyUnicodeError::new_err(text),
        "ValueError" => e::PyValueError::new_err(text),
        _ => e::PyValueError::new_err(message),
    }
}

fn tokenize_inner(sql: String) -> Result<Vec<serde_json::Value>, String> {
    let out = port::tokenize(sql)?;
    Ok(out)
}

/// `tokenize`.
#[pyfunction]
#[pyo3(name = "tokenize", signature = (sql))]
fn tokenize(py: Python<'_>, sql: String) -> PyResult<Py<PyAny>> {
    tokenize_inner(sql)
        .map_err(error)
        .and_then(|v| crate::pyconv::ToPy::to_py(&v, py))
}

fn split_inner(sql: String, strip_semicolon: bool) -> Result<Vec<String>, String> {
    let out = port::split(sql, strip_semicolon)?;
    Ok(out)
}

/// `split`.
#[pyfunction]
#[pyo3(name = "split", signature = (sql, strip_semicolon=false))]
fn split(sql: String, strip_semicolon: bool) -> PyResult<Vec<String>> {
    split_inner(sql, strip_semicolon).map_err(error)
}

fn format_inner(
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
    let out = port::format(
        sql,
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
    )?;
    Ok(out)
}

/// `format`.
#[pyfunction]
#[pyo3(name = "format", signature = (sql, keyword_case=None, identifier_case=None, strip_comments=false, use_space_around_operators=false, strip_whitespace=false, truncate_strings=None, truncate_char=String::from("[...]"), reindent=false, reindent_aligned=false, indent_columns=false, indent_after_first=false, indent_tabs=false, indent_width=2, wrap_after=0, comma_first=false, compact=false, right_margin=None, output_format=None))]
fn format(
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
) -> PyResult<String> {
    format_inner(
        sql,
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
    )
    .map_err(error)
}

#[pymodule]
#[pyo3(name = "_native")]
fn native_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(tokenize, m)?)?;
    m.add_function(wrap_pyfunction!(split, m)?)?;
    m.add_function(wrap_pyfunction!(format, m)?)?;
    Ok(())
}
