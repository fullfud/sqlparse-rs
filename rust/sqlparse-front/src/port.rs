//! The Rust port of `front/sqlparse_front.py`.
//!
//! Replace each `unimplemented!` with the port of the source function. Return `Err` where
//! the source throws or raises: the caller gets an error there too. After changing a
//! signature, run `rust-anything refresh` to regenerate the bindings. `rust-anything diff` checks
//! behavior against what the source recorded.

#![forbid(unsafe_code)]

/// Port of `tokenize` (`front/sqlparse_front.py:13`).
pub fn tokenize(sql: String) -> Result<Vec<serde_json::Value>, String> {
    let _ = &sql;
    unimplemented!("port `tokenize` from front/sqlparse_front.py")
}

/// Port of `split` (`front/sqlparse_front.py:18`).
///
/// `strip_semicolon` defaults to `False` in the source.
pub fn split(sql: String, strip_semicolon: bool) -> Result<Vec<String>, String> {
    let _ = (&sql, &strip_semicolon);
    unimplemented!("port `split` from front/sqlparse_front.py")
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
    let _ = (
        &sql,
        &keyword_case,
        &identifier_case,
        &strip_comments,
        &use_space_around_operators,
        &strip_whitespace,
        &truncate_strings,
        &truncate_char,
        &reindent,
        &reindent_aligned,
        &indent_columns,
        &indent_after_first,
        &indent_tabs,
        &indent_width,
        &wrap_after,
        &comma_first,
        &compact,
        &right_margin,
        &output_format,
    );
    unimplemented!("port `format` from front/sqlparse_front.py")
}
