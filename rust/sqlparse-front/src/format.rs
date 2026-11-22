//! Port of `sqlparse.format` (sqlparse 0.6.1): `formatter.validate_options`, `build_filter_stack`, the token
//! filters (`filters/tokens.py`), the output filters (`filters/output.py`) and `SerializerUnicode`.
//!
//! Options that need the grouped statement tree (strip_comments, strip_whitespace, reindent, reindent_aligned,
//! use_space_around_operators, right_margin) are not ported yet and return an error saying so.

use super::split::{is_a, statements};
use super::{lexer, pystr};

pub(crate) struct Options {
    pub keyword_case: Option<String>,
    pub identifier_case: Option<String>,
    pub strip_comments: bool,
    pub use_space_around_operators: bool,
    pub strip_whitespace: bool,
    pub truncate_strings: Option<i64>,
    pub truncate_char: String,
    pub reindent: bool,
    pub reindent_aligned: bool,
    pub indent_columns: bool,
    #[allow(dead_code)] // read by the reindent filter, not ported yet
    pub indent_after_first: bool,
    #[allow(dead_code)] // read by the reindent filter, not ported yet
    pub indent_tabs: bool,
    pub indent_width: i64,
    pub wrap_after: i64,
    #[allow(dead_code)] // read by the reindent filter, not ported yet
    pub comma_first: bool,
    #[allow(dead_code)] // read by the reindent filter, not ported yet
    pub compact: bool,
    pub right_margin: Option<i64>,
    pub output_format: Option<String>,
}

fn sqlparse_error(msg: String) -> String {
    format!("SQLParseError: {msg}")
}

#[derive(Clone, Copy)]
enum Case {
    Upper,
    Lower,
    Capitalize,
}

fn case_of(name: &Option<String>, option: &str) -> Result<Option<Case>, String> {
    match name.as_deref() {
        None => Ok(None),
        Some("upper") => Ok(Some(Case::Upper)),
        Some("lower") => Ok(Some(Case::Lower)),
        Some("capitalize") => Ok(Some(Case::Capitalize)),
        Some(other) => Err(sqlparse_error(format!(
            "Invalid value for {option}: {}",
            pystr::repr(other)
        ))),
    }
}

fn convert(case: Case, value: &str) -> String {
    match case {
        Case::Upper => pystr::upper(value),
        Case::Lower => pystr::lower(value),
        Case::Capitalize => pystr::capitalize(value),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Output {
    Python,
    Php,
}

/// The validated options that decide the pipeline (`validate_options`, in its order and with its messages).
struct Plan {
    keyword_case: Option<Case>,
    identifier_case: Option<Case>,
    truncate: Option<(usize, String)>,
    grouping: bool,
    output: Option<Output>,
}

fn validate(o: &Options) -> Result<Plan, String> {
    let keyword_case = case_of(&o.keyword_case, "keyword_case")?;
    let identifier_case = case_of(&o.identifier_case, "identifier_case")?;
    let output = match o.output_format.as_deref() {
        None | Some("sql") => None,
        Some("python") => Some(Output::Python),
        Some("php") => Some(Output::Php),
        Some(other) => {
            return Err(sqlparse_error(format!(
                "Unknown output format: {}",
                pystr::repr(other)
            )));
        }
    };
    let truncate = match o.truncate_strings {
        None => None,
        Some(w) if w <= 1 => {
            return Err(sqlparse_error(format!(
                "Invalid value for truncate_strings: {w}"
            )));
        }
        Some(w) => Some((
            usize::try_from(w).unwrap_or(usize::MAX),
            o.truncate_char.clone(),
        )),
    };
    let reindent = o.reindent || o.indent_columns;
    let strip_whitespace = o.strip_whitespace || reindent || o.reindent_aligned;
    if o.indent_width < 1 {
        return Err(sqlparse_error(
            "indent_width requires a positive integer".into(),
        ));
    }
    if o.wrap_after < 0 {
        return Err(sqlparse_error(
            "wrap_after requires a positive integer".into(),
        ));
    }
    if let Some(m) = o.right_margin
        && m < 10
    {
        return Err(sqlparse_error(
            "right_margin requires an integer > 10".into(),
        ));
    }
    let grouping = o.use_space_around_operators
        || o.strip_comments
        || strip_whitespace
        || reindent
        || o.reindent_aligned
        || o.right_margin.is_some();
    Ok(Plan {
        keyword_case,
        identifier_case,
        truncate,
        grouping,
        output,
    })
}

/// Python's `value[a:b]` on code points, for the non-negative and negative bounds TruncateStringFilter uses.
fn py_slice(chars: &[char], start: isize, end: isize) -> String {
    let n = chars.len() as isize;
    let norm = |i: isize| if i < 0 { (n + i).max(0) } else { i.min(n) };
    let (a, b) = (norm(start), norm(end));
    if a >= b {
        String::new()
    } else {
        chars[a as usize..b as usize].iter().collect()
    }
}

fn truncate(value: &str, width: usize, fill: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let (inner, quote) = if py_slice(&chars, 0, 2) == "''" {
        (py_slice(&chars, 2, -2), "''")
    } else {
        (py_slice(&chars, 1, -1), "'")
    };
    if inner.chars().count() > width {
        let head: String = inner.chars().take(width).collect();
        format!("{quote}{head}{fill}{quote}")
    } else {
        value.to_string()
    }
}

/// `utils.split_unquoted_newlines`.
fn split_unquoted_newlines(text: &str) -> Vec<&str> {
    let b = text.as_bytes();
    let mut lines = Vec::new();
    let (mut line_start, mut pos) = (0, 0);
    let mut unclosed_dq = 0usize;
    let mut unclosed_sq = 0usize;
    while pos < b.len() {
        match b[pos] {
            b'\r' | b'\n' => {
                lines.push(&text[line_start..pos]);
                pos += if text[pos..].starts_with("\r\n") {
                    2
                } else {
                    1
                };
                line_start = pos;
            }
            q @ (b'"' | b'\'') => {
                let unclosed = if q == b'"' {
                    &mut unclosed_dq
                } else {
                    &mut unclosed_sq
                };
                if pos >= *unclosed {
                    let end = string_content_end(text, pos + 1, q);
                    if end < b.len() && b[end] == q {
                        pos = end + 1;
                        continue;
                    }
                    *unclosed = end;
                }
                pos += 1;
            }
            _ => {
                while pos < b.len() && !matches!(b[pos], b'\r' | b'\n' | b'"' | b'\'') {
                    pos += 1;
                }
            }
        }
    }
    lines.push(&text[line_start..]);
    lines
}

/// `(?:[^q\\]|\\.)*` from `start`: where a string's content ends (`.` does not match a newline).
fn string_content_end(text: &str, start: usize, q: u8) -> usize {
    let b = text.as_bytes();
    let mut pos = start;
    while pos < b.len() {
        if b[pos] == b'\\' {
            match text[pos + 1..].chars().next() {
                Some(c) if c != '\n' => pos += 1 + c.len_utf8(),
                _ => break,
            }
        } else if b[pos] == q {
            break;
        } else {
            pos += 1;
        }
    }
    pos
}

const WHITESPACE: &str = "Token.Text.Whitespace";

/// `OutputPythonFilter` / `OutputPHPFilter._process` on one statement's top-level tokens.
fn output_filter(
    kind: Output,
    tokens: Vec<(&'static str, String)>,
    count: usize,
) -> Vec<(&'static str, String)> {
    let base = if kind == Output::Php { "$sql" } else { "sql" };
    let varname = if count > 1 {
        format!("{base}{count}")
    } else {
        base.to_string()
    };
    let text: String = tokens.iter().map(|(_, v)| v.as_str()).collect();
    let has_nl = pystr::splitlines_count(pystr::strip(&text)) > 1;
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let mut put = |t: &'static str, v: &str| out.push((t, v.to_string()));
    if count > 1 {
        put(WHITESPACE, "\n");
    }
    put("Token.Name", &varname);
    put(WHITESPACE, " ");
    if kind == Output::Php && has_nl {
        put(WHITESPACE, " ");
    }
    put("Token.Operator", "=");
    put(WHITESPACE, " ");
    if kind == Output::Python && has_nl {
        put("Token.Operator", "(");
    }
    put("Token.Text", if kind == Output::Php { "\"" } else { "'" });
    for (ttype, value) in tokens {
        if is_a(ttype, WHITESPACE) && value.contains('\n') {
            match kind {
                Output::Python => {
                    put("Token.Text", " '");
                    put(WHITESPACE, "\n");
                    put(WHITESPACE, &" ".repeat(varname.chars().count() + 4));
                    put("Token.Text", "'");
                }
                Output::Php => {
                    put("Token.Text", " \";");
                    put(WHITESPACE, "\n");
                    put("Token.Name", &varname);
                    put(WHITESPACE, " ");
                    put("Token.Operator", ".=");
                    put(WHITESPACE, " ");
                    put("Token.Text", "\"");
                }
            }
            let after_lb = value.split_once('\n').map_or("", |(_, rest)| rest);
            if !after_lb.is_empty() {
                put(WHITESPACE, after_lb);
            }
            continue;
        }
        let escaped = match kind {
            Output::Python => value.replace('\\', "\\\\").replace('\'', "\\'"),
            Output::Php => value
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('$', "\\$"),
        };
        put("Token.Text", &escaped);
    }
    match kind {
        Output::Python => {
            put("Token.Text", "'");
            if has_nl {
                put("Token.Operator", ")");
            }
        }
        Output::Php => {
            put("Token.Text", "\"");
            put("Token.Punctuation", ";");
        }
    }
    out
}

/// `sqlparse.format(text, **options)`.
pub(crate) fn format(text: &str, o: &Options) -> Result<String, String> {
    let plan = validate(o)?;
    if plan.grouping {
        return Err(
            "NotImplementedError: format options that group the statement are not ported yet"
                .into(),
        );
    }
    let mut tokens: Vec<(&'static str, String)> = lexer::tokenize(text)
        .into_iter()
        .map(|(t, v)| (t, v.to_string()))
        .collect();
    if let Some(case) = plan.keyword_case {
        for (t, v) in &mut tokens {
            if is_a(t, "Token.Keyword") {
                *v = convert(case, v);
            }
        }
    }
    if let Some(case) = plan.identifier_case {
        for (t, v) in &mut tokens {
            if (*t == "Token.Name" || *t == "Token.Literal.String.Symbol")
                && !pystr::strip(v).starts_with('"')
            {
                *v = convert(case, v);
            }
        }
    }
    if let Some((width, fill)) = &plan.truncate {
        for (t, v) in &mut tokens {
            if *t == "Token.Literal.String.Single" {
                *v = truncate(v, *width, fill);
            }
        }
    }
    let view: Vec<(&'static str, &str)> = tokens.iter().map(|(t, v)| (*t, v.as_str())).collect();
    let mut out = String::with_capacity(text.len());
    for (count, range) in statements(&view).into_iter().enumerate() {
        let mut stmt: Vec<(&'static str, String)> = tokens[range].to_vec();
        if let Some(kind) = plan.output {
            stmt = output_filter(kind, stmt, count + 1);
        }
        let joined: String = stmt.iter().map(|(_, v)| v.as_str()).collect();
        let lines: Vec<&str> = split_unquoted_newlines(&joined)
            .into_iter()
            .map(pystr::rstrip)
            .collect();
        out.push_str(&lines.join("\n"));
    }
    Ok(out)
}
