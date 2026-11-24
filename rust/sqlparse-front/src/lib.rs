//! Python bindings for `front/sqlparse_front.py`: generated from the signatures in `port.rs` by rust-anything.
//! Regenerate with `rust-anything refresh` after changing them.

#![allow(clippy::too_many_arguments)]

use pyo3::prelude::*;

pub mod js;
pub mod port;
pub mod pyconv;
pub mod pynum;
pub mod slots;

#[allow(unused_imports)]
use pynum::Num;

/// The port allocates and frees many small vectors (a token list per group, a tree per statement); mimalloc
/// does that faster than the system allocators (6-20 % on the benchmarks, most on Windows). It serves the
/// Rust's own allocations only: Python's objects are still made by Python.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pyo3::create_exception!(
    _native,
    Deep,
    pyo3::exceptions::PyException,
    "A statement the Rust hands to sqlparse: deep enough, from a caller near Python's recursion limit, that a      grouping pass which does not count frames could reach the limit."
);
pyo3::create_exception!(
    _native,
    Oversize,
    pyo3::exceptions::PyException,
    "A call the Rust hands to sqlparse: its output grows past what the Rust makes without risk."
);

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
    // the package's own hand-overs: made here, with no Python call (they come near the recursion limit)
    match kind {
        "sqlparse_turbo._Deep" => return Deep::new_err(text.to_string()),
        "sqlparse_turbo._Oversize" => return Oversize::new_err(text.to_string()),
        _ => {}
    }
    // `raise NotImplementedError`, a failed allocation's MemoryError: no arguments, not an empty message
    if text.is_empty()
        && let Some(err) = Python::attach(|py| -> Option<PyErr> {
            let class = py.import("builtins").ok()?.getattr(kind).ok()?;
            let class = class.cast_into::<pyo3::types::PyType>().ok()?;
            if !class
                .is_subclass_of::<pyo3::exceptions::PyException>()
                .ok()?
            {
                return None;
            }
            Some(PyErr::from_value(class.call0().ok()?))
        })
    {
        return err;
    }
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
    let (types, layout) = Python::attach(|py| -> PyResult<_> {
        let types = types
            .iter()
            .map(|(k, v)| (k.clone(), v.clone_ref(py)))
            .collect();
        let dict = pyo3::types::PyDict::new(py);
        for (k, v) in &classes {
            dict.set_item(k, v.bind(py))?;
        }
        Ok((types, slots::Layout::find(py, &dict)))
    })?;
    let _ = SQL_OBJECTS.set(port::SqlObjects {
        classes,
        types,
        new,
        layout,
    });
    Ok(())
}

/// `objects_direct`: whether sqlparse's objects are made and filled directly (see `slots`), not through
/// `object.__new__` and `setattr` calls. For the checks.
#[pyfunction]
#[pyo3(name = "objects_direct")]
fn objects_direct() -> bool {
    SQL_OBJECTS.get().is_some_and(|o| o.layout.is_some())
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
    let statements = gc_paused(py, || parsed.to_python(py, objects))?;
    pyo3::types::PyTuple::new(py, statements)
}

/// `gc.isenabled`, `gc.disable`, `gc.enable`, as the package found them.
static GC: std::sync::OnceLock<Option<[Py<PyAny>; 3]>> = std::sync::OnceLock::new();

/// Python's cyclic garbage collector held off while sqlparse's objects are made. A tree is thousands of new
/// objects, each of which counts towards the next collection, and each collection walks them all again: without
/// this, collecting took two thirds of `parse()`. Nothing else runs meanwhile (no Python code is called while the
/// objects are made, so no other thread gets the GIL), and the collector is on again before the objects are
/// handed over, as it was; the next collection sees them once.
fn gc_paused<'py, T>(py: Python<'py>, make: impl FnOnce() -> PyResult<T>) -> PyResult<T> {
    struct On<'a, 'py>(Python<'py>, &'a Py<PyAny>);
    impl Drop for On<'_, '_> {
        fn drop(&mut self) {
            let _ = self.1.bind(self.0).call0();
        }
    }
    let gc = GC.get_or_init(|| {
        let gc = py.import("gc").ok()?;
        Some([
            gc.getattr("isenabled").ok()?.unbind(),
            gc.getattr("disable").ok()?.unbind(),
            gc.getattr("enable").ok()?.unbind(),
        ])
    });
    let Some([isenabled, disable, enable]) = gc else {
        return make();
    };
    if !isenabled.bind(py).call0()?.is_truthy()? {
        return make();
    }
    disable.bind(py).call0()?;
    let _on = On(py, enable);
    make()
}

/// `statements`: a caller's `FilterStack.run` with grouping, one statement at a time (see `port::Lazy`); None if
/// the text cannot be taken (never expected).
#[pyfunction]
#[pyo3(name = "statements", signature = (sql))]
fn statements(sql: String) -> Option<Statements> {
    port::Lazy::new(sql).map(Statements)
}

#[pyclass(name = "Statements", module = "sqlparse_turbo._native")]
struct Statements(port::Lazy);

#[pymethods]
impl Statements {
    /// The next statement as sqlparse's own `Statement`, grouped now; whether the caller's first statement filter,
    /// an `AlignedIndentFilter` whose state `aligned` gives or a `ReindentFilter` whose state `reindent` gives, was
    /// run on it here; and the token that `ReindentFilter` set its `_last_func` to (None: not set). None after the
    /// last statement.
    #[pyo3(signature = (aligned=None, reindent=None))]
    #[allow(clippy::type_complexity)]
    fn next<'py>(
        &mut self,
        py: Python<'py>,
        aligned: Option<port::AlignedState>,
        reindent: Option<port::ReindentState>,
    ) -> PyResult<Option<(Bound<'py, PyAny>, bool, Option<Bound<'py, PyAny>>)>> {
        let objects = SQL_OBJECTS.get().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("statements: set_sql_objects was not called")
        })?;
        let Some(grouped) = self.0.next(aligned, reindent).map_err(error)? else {
            return Ok(None);
        };
        let (statement, last_func) = gc_paused(py, || grouped.to_python(py, objects))?;
        Ok(Some((statement, grouped.filtered(), last_func)))
    }

    /// Takes the next statement without grouping it, for a stack whose grouping was turned off since the
    /// first statement (`last_tokens` then gives it); false after the last.
    fn skip(&mut self) -> bool {
        self.0.skip()
    }

    /// The tokens of the statement `next` last took, as (token type object, text) pairs.
    fn last_tokens<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyList>> {
        use pyo3::types::{PyString, PyTuple};
        let types = TOKEN_TYPES.get().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("last_tokens: set_token_types was not called")
        })?;
        let mut items = Vec::new();
        for (ttype, value) in self.0.last_tokens() {
            let t = types
                .get(ttype)
                .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(ttype))?;
            items.push(PyTuple::new(
                py,
                [t.bind(py).clone(), PyString::new(py, value).into_any()],
            )?);
        }
        pyo3::types::PyList::new(py, items)
    }
}

/// One watched namespace: a module's `__dict__` or a class's own dict, whether it must keep its size (a class:
/// a new attribute may override an inherited one; a module gains a name when a submodule is first imported),
/// and its entries in order as the package found them: the key, the objects allowed as the value (None: any),
/// and whether an equal value of the same type will do (a number or string put back as a new object).
type WatchEntry = (Py<PyAny>, Option<Vec<Py<PyAny>>>, bool);
type Watched = (Py<pyo3::types::PyDict>, bool, Vec<WatchEntry>);

static WATCH: std::sync::OnceLock<Vec<Watched>> = std::sync::OnceLock::new();

/// Changes the dict watcher has seen in sqlparse's namespaces, and the count `untouched_watched` last found
/// them unchanged at (none yet: `u64::MAX`).
static CHANGES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CHECKED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
/// The watcher id the package got from `PyDict_AddWatcher` (kept here, across a reload of the package).
static WATCHER: std::sync::OnceLock<i32> = std::sync::OnceLock::new();

/// The dict watcher's callback (CPython 3.12+ `PyDict_WatchCallback`): counts a change, before the dict
/// changes, within the same C call. It runs no Python code, so no other thread runs between the count and the
/// change, and it is safe at interpreter shutdown.
extern "C" fn on_dict_change(
    _event: std::os::raw::c_int,
    _dict: *mut pyo3::ffi::PyObject,
    _key: *mut pyo3::ffi::PyObject,
    _value: *mut pyo3::ffi::PyObject,
) -> std::os::raw::c_int {
    CHANGES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    0
}

/// `watch_callback`: the address of the callback, for the package to hand to `PyDict_AddWatcher`.
#[pyfunction]
#[pyo3(name = "watch_callback")]
fn watch_callback() -> usize {
    on_dict_change as *const () as usize
}

/// `watcher` / `set_watcher`: the watcher id, once the package has one.
#[pyfunction]
#[pyo3(name = "watcher")]
fn watcher() -> Option<i32> {
    WATCHER.get().copied()
}

#[pyfunction]
#[pyo3(name = "set_watcher", signature = (id))]
fn set_watcher(id: i32) {
    let _ = WATCHER.set(id);
}

/// `untouched_watched`: `untouched()`, compared again only when the watcher has seen a change since it last
/// held. A patch that stays in place is compared on every call (and found), so no call slips through to the
/// Rust while it is there.
#[pyfunction]
#[pyo3(name = "untouched_watched")]
fn untouched_watched(py: Python<'_>) -> bool {
    use std::sync::atomic::Ordering::SeqCst;
    let seen = CHANGES.load(SeqCst);
    if CHECKED.load(SeqCst) == seen {
        return true;
    }
    if !untouched(py) {
        return false;
    }
    CHECKED.store(seen, SeqCst);
    true
}

/// `same_items`: whether two lists hold the very same objects, in order (the lexer's rules against the ones
/// found to be sqlparse's: a rule replaced in place shows).
#[pyfunction]
#[pyo3(name = "same_items", signature = (a, b))]
fn same_items(a: &Bound<'_, pyo3::types::PyList>, b: &Bound<'_, pyo3::types::PyList>) -> bool {
    use pyo3::types::PyListMethods;
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.is(&y))
}

/// sqlparse's `Lexer` class, the keyword dicts its default instance holds as sqlparse sets it up, and the rules
/// last found to be sqlparse's: what `lexer_ported` compares against.
struct LexerRef {
    cls: Py<PyAny>,
    keywords: Vec<Py<PyAny>>,
    rules: Vec<Py<PyAny>>,
}

static LEXER_REF: std::sync::Mutex<Option<std::sync::Arc<LexerRef>>> = std::sync::Mutex::new(None);

/// `set_lexer_reference`: the package hands over the `Lexer` class, its default keyword dicts and the rules found
/// to be sqlparse's (again after it found other rules equal by content).
#[pyfunction]
#[pyo3(name = "set_lexer_reference", signature = (cls, keywords, rules))]
fn set_lexer_reference(cls: Py<PyAny>, keywords: Vec<Py<PyAny>>, rules: Vec<Py<PyAny>>) {
    if let Ok(mut r) = LEXER_REF.lock() {
        *r = Some(std::sync::Arc::new(LexerRef {
            cls,
            keywords,
            rules,
        }));
    }
}

/// `lexer_ported`: the default lexer's `_keywords` and `_SQL_REGEX`, both lists, against the reference, object
/// by object: True when both hold the very same objects, False when the keyword dicts differ; None when the
/// comparison cannot tell (no default instance yet, something other than a list, rules other than those last
/// found), for the package to look closer.
#[pyfunction]
#[pyo3(name = "lexer_ported")]
fn lexer_ported(py: Python<'_>) -> Option<bool> {
    use pyo3::intern;
    use pyo3::types::{PyList, PyListMethods};
    // the reference taken out first: the lookups below may run Python code, which may call this again
    let r = LEXER_REF.lock().ok()?.clone()?;
    let inst = r
        .cls
        .bind(py)
        .getattr(intern!(py, "_default_instance"))
        .ok()?;
    if inst.is_none() {
        return None;
    }
    // item by item by address, without taking a reference to each (the rules are some eighty)
    let same = |list: &Bound<'_, PyList>, refs: &[Py<PyAny>]| {
        list.len() == refs.len()
            && refs.iter().enumerate().all(|(i, r)| {
                // SAFETY: `list` is a live list held here and `i` is below its length, read under the GIL with
                // no Python code run in between; the borrowed pointer is only compared, never kept or used.
                unsafe { pyo3::ffi::PyList_GetItem(list.as_ptr(), i as isize) == r.as_ptr() }
            })
    };
    let kws = inst.getattr(intern!(py, "_keywords")).ok()?;
    let kws = kws.cast_exact::<PyList>().ok()?;
    if !same(kws, &r.keywords) {
        return Some(false);
    }
    let rx = inst.getattr(intern!(py, "_SQL_REGEX")).ok()?;
    let rx = rx.cast_exact::<PyList>().ok()?;
    same(rx, &r.rules).then_some(true)
}

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
    m.add_function(wrap_pyfunction!(statements, m)?)?;
    m.add_function(wrap_pyfunction!(objects_direct, m)?)?;
    m.add_function(wrap_pyfunction!(watch_callback, m)?)?;
    m.add_function(wrap_pyfunction!(watcher, m)?)?;
    m.add_function(wrap_pyfunction!(set_watcher, m)?)?;
    m.add_function(wrap_pyfunction!(untouched_watched, m)?)?;
    m.add_function(wrap_pyfunction!(same_items, m)?)?;
    m.add_function(wrap_pyfunction!(set_lexer_reference, m)?)?;
    m.add_function(wrap_pyfunction!(lexer_ported, m)?)?;
    m.add("Deep", m.py().get_type::<Deep>())?;
    m.add("Oversize", m.py().get_type::<Oversize>())?;
    m.add_class::<Statements>()?;
    Ok(())
}
