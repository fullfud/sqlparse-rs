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
    /// `Err("Kind: text")` for a `Kind` the match below does not know: a built-in exception's name or
    /// `package.module.Class`, called with the text. None when it names no `Exception` subclass or the
    /// class cannot be made from the text alone (its constructor takes more arguments).
    fn named_error(kind: &str, text: &str) -> Option<PyErr> {
        Python::attach(|py| {
            // Each dotted part must be a Python identifier, by Python's own rules (`str.isidentifier`).
            let identifier = |part: &str| {
                pyo3::types::PyString::new(py, part)
                    .call_method0("isidentifier")
                    .and_then(|ok| ok.extract::<bool>())
                    .unwrap_or(false)
            };
            if !kind.split('.').all(identifier) {
                return None;
            }
            let (module, name) = kind.rsplit_once('.').unwrap_or(("builtins", kind));
            let class = py.import(module).ok()?.getattr(name).ok()?;
            let class = class.cast_into::<pyo3::types::PyType>().ok()?;
            if !class
                .is_subclass_of::<pyo3::exceptions::PyException>()
                .ok()?
            {
                return None;
            }
            let value = class.call1((text,)).ok()?;
            if !value.is_instance(&class).ok()? {
                return None;
            }
            Some(PyErr::from_value(value))
        })
    }
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
        _ => named_error(kind, &text).unwrap_or_else(|| e::PyValueError::new_err(message)),
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

/// `split`. Other Python threads run meanwhile (split reads nothing from Python).
#[pyfunction]
#[pyo3(name = "split", signature = (sql, strip_semicolon=false))]
fn split(py: Python<'_>, sql: String, strip_semicolon: bool) -> PyResult<Vec<String>> {
    py.detach(|| split_inner(sql, strip_semicolon))
        .map_err(error)
}

/// `split_many`: not in the front door; `split` of each text, the texts shared out over the CPU's threads
/// with Python's lock released.
#[pyfunction]
#[pyo3(name = "split_many", signature = (sqls, strip_semicolon=false))]
fn split_many(
    py: Python<'_>,
    sqls: Vec<String>,
    strip_semicolon: bool,
) -> PyResult<Vec<Vec<String>>> {
    py.detach(|| {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        let chunk = sqls.len().div_ceil(threads).max(1);
        std::thread::scope(|scope| {
            let parts: Vec<_> = sqls
                .chunks(chunk)
                .map(|part| {
                    scope.spawn(move || {
                        part.iter()
                            .map(|sql| port::split(sql.clone(), strip_semicolon))
                            .collect::<Result<Vec<_>, String>>()
                    })
                })
                .collect();
            let mut out = Vec::with_capacity(sqls.len());
            for part in parts {
                out.extend(
                    part.join()
                        .map_err(|_| "split_many: a worker panicked".to_string())??,
                );
            }
            Ok(out)
        })
    })
    .map_err(error)
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

fn parse_tree_inner(sql: String) -> Result<Vec<serde_json::Value>, String> {
    let out = port::parse_tree(sql)?;
    Ok(out)
}

/// `parse_tree`.
#[pyfunction]
#[pyo3(name = "parse_tree", signature = (sql))]
fn parse_tree(py: Python<'_>, sql: String) -> PyResult<Py<PyAny>> {
    parse_tree_inner(sql)
        .map_err(error)
        .and_then(|v| crate::pyconv::ToPy::to_py(&v, py))
}

/// The token type objects of `sqlparse.tokens` by name, handed over once by the package for `lex`.
static TOKEN_TYPES: std::sync::OnceLock<std::collections::HashMap<String, Py<PyAny>>> =
    std::sync::OnceLock::new();

/// `token_type_names`: every token type name `lex` can give.
#[pyfunction]
#[pyo3(name = "token_type_names")]
fn token_type_names() -> Vec<&'static str> {
    port::token_type_names()
}

/// `set_token_types`: the `sqlparse.tokens` objects for `token_type_names()`.
#[pyfunction]
#[pyo3(name = "set_token_types", signature = (types))]
fn set_token_types(types: std::collections::HashMap<String, Py<PyAny>>) {
    let _ = TOKEN_TYPES.set(types);
}

/// `lex`: `sqlparse.lexer.tokenize(text)` as a list of (token type object, text) tuples.
#[pyfunction]
#[pyo3(name = "lex", signature = (text))]
fn lex<'py>(py: Python<'py>, text: String) -> PyResult<Bound<'py, pyo3::types::PyList>> {
    use pyo3::types::{PyString, PyTuple};
    let types = TOKEN_TYPES.get().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("lex: set_token_types was not called")
    })?;
    let tokens = port::lex(&text);
    let mut items = Vec::with_capacity(tokens.len());
    // the lexer's type names are a handful of static strings: find each by address first, by name once
    let mut seen: Vec<(usize, &Py<PyAny>)> = Vec::with_capacity(16);
    for (ttype, value) in tokens {
        let addr = ttype.as_ptr() as usize;
        let t = match seen.iter().find(|(a, _)| *a == addr) {
            Some(&(_, t)) => t,
            None => {
                let t = types
                    .get(ttype)
                    .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(ttype))?;
                seen.push((addr, t));
                t
            }
        };
        items.push(PyTuple::new(
            py,
            [t.bind(py).clone(), PyString::new(py, value).into_any()],
        )?);
    }
    pyo3::types::PyList::new(py, items)
}

/// What `parse` builds sqlparse's objects from, handed over once by the package.
static SQL_OBJECTS: std::sync::OnceLock<port::SqlObjects> = std::sync::OnceLock::new();

/// `set_sql_objects`: `sqlparse.sql`'s classes by name and `object.__new__` (the token types come from
/// `set_token_types`, which must be called first).
#[pyfunction]
#[pyo3(name = "set_sql_objects", signature = (classes, new))]
fn set_sql_objects(
    classes: std::collections::HashMap<String, Py<PyAny>>,
    new: Py<PyAny>,
) -> PyResult<()> {
    let types = TOKEN_TYPES.get().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("set_sql_objects: call set_token_types first")
    })?;
    let types = Python::attach(|py| {
        types
            .iter()
            .map(|(k, v)| (k.clone(), v.clone_ref(py)))
            .collect()
    });
    let _ = SQL_OBJECTS.set(port::SqlObjects {
        classes,
        types,
        new,
    });
    Ok(())
}

/// `parse`: `sqlparse.parse(sql)`, grouped in Rust, as a tuple of sqlparse's own `Statement` objects. With
/// `run`, the recursion limit is counted as for sqlparse's `FilterStack.run` (called from the package's).
#[pyfunction]
#[pyo3(name = "parse", signature = (sql, run=false))]
fn parse<'py>(
    py: Python<'py>,
    sql: String,
    run: bool,
) -> PyResult<Bound<'py, pyo3::types::PyTuple>> {
    let objects = SQL_OBJECTS.get().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("parse: set_sql_objects was not called")
    })?;
    let parsed = port::parse_trees(&sql, run).map_err(error)?;
    let statements = parsed.to_python(py, objects);
    pyo3::types::PyTuple::new(py, statements?)
}

/// One watched namespace: a module's `__dict__` or a class's own dict, whether it must keep its size (a class:
/// a new attribute may override an inherited one; a module gains a name when a submodule is first imported),
/// and its entries in order as the package found them: the key, the objects allowed as the value (None: any),
/// and whether an equal value of the same type will do (a number or string put back as a new object).
type WatchEntry = (Py<PyAny>, Option<Vec<Py<PyAny>>>, bool);
type Watched = (Py<pyo3::types::PyDict>, bool, Vec<WatchEntry>);

static WATCH: std::sync::OnceLock<Vec<Watched>> = std::sync::OnceLock::new();

/// `set_watch`: sqlparse's namespaces as the package found them.
#[pyfunction]
#[pyo3(name = "set_watch", signature = (watch))]
fn set_watch(watch: Vec<Watched>) {
    let _ = WATCH.set(watch);
}

/// `untouched`: whether sqlparse's namespaces still hold what they held: the same keys in the same order with
/// allowed values (a replaced, removed or re-added name shows), a class nothing more. One pass over each dict.
#[pyfunction]
#[pyo3(name = "untouched")]
fn untouched(py: Python<'_>) -> bool {
    use pyo3::types::PyDictMethods;
    let Some(watch) = WATCH.get() else {
        return false;
    };
    watch.iter().all(|(dict, fixed_size, entries)| {
        let d = dict.bind(py);
        let len = d.len();
        if len < entries.len() || (*fixed_size && len != entries.len()) {
            return false;
        }
        d.iter()
            .zip(entries)
            .all(|((k, v), (key, allowed, by_value))| {
                k.as_ptr() == key.as_ptr()
                    && allowed.as_ref().is_none_or(|a| {
                        a.iter().any(|x| x.as_ptr() == v.as_ptr())
                            || (*by_value
                                && a.iter().any(|x| {
                                    let x = x.bind(py);
                                    x.get_type().is(v.get_type()) && x.eq(&v).unwrap_or(false)
                                }))
                    })
            })
    })
}

/// `set_drift`: not in the front door; the package's Unicode-version guard (see `port::set_drift`).
#[pyfunction]
#[pyo3(name = "set_drift", signature = (ranges))]
fn set_drift(ranges: Vec<(u32, u32)>) {
    port::set_drift(&ranges)
}

/// `drifted`: whether the text holds a code point of the drift list.
#[pyfunction]
#[pyo3(name = "drifted", signature = (text))]
fn drifted(text: String) -> bool {
    port::drifted(&text)
}

#[pymodule]
#[pyo3(name = "_native")]
fn native_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(tokenize, m)?)?;
    m.add_function(wrap_pyfunction!(split, m)?)?;
    m.add_function(wrap_pyfunction!(format, m)?)?;
    m.add_function(wrap_pyfunction!(parse_tree, m)?)?;
    m.add_function(wrap_pyfunction!(set_drift, m)?)?;
    m.add_function(wrap_pyfunction!(split_many, m)?)?;
    m.add_function(wrap_pyfunction!(token_type_names, m)?)?;
    m.add_function(wrap_pyfunction!(set_token_types, m)?)?;
    m.add_function(wrap_pyfunction!(lex, m)?)?;
    m.add_function(wrap_pyfunction!(set_sql_objects, m)?)?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(set_watch, m)?)?;
    m.add_function(wrap_pyfunction!(untouched, m)?)?;
    m.add_function(wrap_pyfunction!(drifted, m)?)?;
    Ok(())
}
