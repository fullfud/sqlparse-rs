"""The front door of the port: sqlparse's string-in, string-out surface as plain typed functions.

rust-anything bridges this file. Each function here calls the original sqlparse (the submodule in upstream/,
pinned to a release) and the Rust crate in rust/ must return exactly the same thing. Strings cross the bridge
once per call; the token tree never does.

Order of work: tokenize (lexer) -> split (statement splitter) -> format (grouping + filters + output).
"""

import sqlparse


def tokenize(sql: str) -> list[tuple[str, str]]:
    """The lexer's output: (token type, text) pairs, the type as sqlparse prints it ('Token.Keyword.DML')."""
    return [(str(ttype), value) for ttype, value in sqlparse.lexer.tokenize(sql)]


def split(sql: str, strip_semicolon: bool = False) -> list[str]:
    """sqlparse.split(): the statements in sql, stripped."""
    return sqlparse.split(sql, strip_semicolon=strip_semicolon)


def format(
    sql: str,
    keyword_case: str | None = None,
    identifier_case: str | None = None,
    strip_comments: bool = False,
    use_space_around_operators: bool = False,
    strip_whitespace: bool = False,
    truncate_strings: int | None = None,
    truncate_char: str = '[...]',
    reindent: bool = False,
    reindent_aligned: bool = False,
    indent_columns: bool = False,
    indent_after_first: bool = False,
    indent_tabs: bool = False,
    indent_width: int = 2,
    wrap_after: int = 0,
    comma_first: bool = False,
    compact: bool = False,
    right_margin: int | None = None,
    output_format: str | None = None,
) -> str:
    """sqlparse.format() with every option it takes; an option left at its default is not passed on."""
    options = {}
    for name, value, default in (
        ('keyword_case', keyword_case, None), ('identifier_case', identifier_case, None),
        ('strip_comments', strip_comments, False), ('use_space_around_operators', use_space_around_operators, False),
        ('strip_whitespace', strip_whitespace, False), ('truncate_strings', truncate_strings, None),
        ('truncate_char', truncate_char, '[...]'), ('reindent', reindent, False),
        ('reindent_aligned', reindent_aligned, False), ('indent_columns', indent_columns, False),
        ('indent_after_first', indent_after_first, False), ('indent_tabs', indent_tabs, False),
        ('indent_width', indent_width, 2), ('wrap_after', wrap_after, 0), ('comma_first', comma_first, False),
        ('compact', compact, False), ('right_margin', right_margin, None), ('output_format', output_format, None),
    ):
        if value != default:
            options[name] = value
    return sqlparse.format(sql, **options)

