"""sqlparse's format() and split(), run in Rust, with sqlparse itself for everything the Rust was not proven on.

    import sqlparse_rs
    sqlparse_rs.format(sql, reindent=True, keyword_case='upper')   # same signature and result as sqlparse.format
    sqlparse_rs.split(sql)

    sqlparse_rs.install()   # sqlparse.format and sqlparse.split now go through here, for every caller
                            # that looks them up on the module (sqlparse.format(...)); a name bound earlier
                            # with `from sqlparse import format` keeps the original

The Rust is a port of sqlparse 0.6.1 checked call by call against it (see the project's README). A call goes to
the original sqlparse, unchanged, unless all of this holds:

- the installed sqlparse is 0.6.1;
- `sql` is a `str` (not bytes, not a subclass) and `encoding` is None;
- sqlparse's default lexer is as sqlparse sets it up (no add_keywords, set_SQL_REGEX, clear);
- every option is one format() knows, with exactly the type the port takes: bool options are `bool` (not 0/1
  or None), `indent_width`/`wrap_after` are `int` (not bool) and `truncate_strings`/`right_margin` `int` or
  None, within 64 bits (`indent_width` at most 4096, see _INDENT_WIDTH_MAX), `keyword_case`/`identifier_case`/`output_format` `str` or None, `truncate_char` `str`;
  split's `strip_semicolon` is a `bool`;
- the text can be encoded as UTF-8 (a str with lone surrogates cannot).

Errors are sqlparse's own: an invalid option value raises `sqlparse.exceptions.SQLParseError` with sqlparse's
message, and so does a statement nested past Python's recursion limit, at the same depth as in sqlparse.
"""

import sqlparse
from sqlparse import lexer as _lexer

from . import _native

__all__ = ['format', 'split', 'install', 'uninstall', 'PORTED_VERSION']

PORTED_VERSION = '0.6.1'

_original_format = sqlparse.format
_original_split = sqlparse.split
_ported = sqlparse.__version__ == PORTED_VERSION

_BOOL, _INT, _OPT_INT, _STR, _OPT_STR = range(5)
_OPTIONS = {
    'keyword_case': _OPT_STR,
    'identifier_case': _OPT_STR,
    'output_format': _OPT_STR,
    'truncate_char': _STR,
    'strip_comments': _BOOL,
    'use_space_around_operators': _BOOL,
    'strip_whitespace': _BOOL,
    'reindent': _BOOL,
    'reindent_aligned': _BOOL,
    'indent_columns': _BOOL,
    'indent_after_first': _BOOL,
    'indent_tabs': _BOOL,
    'comma_first': _BOOL,
    'compact': _BOOL,
    'indent_width': _INT,
    'wrap_after': _INT,
    'truncate_strings': _OPT_INT,
    'right_margin': _OPT_INT,
}
_I64_MIN = -(2**63)
_I64_MAX = 2**63 - 1
# Every new line gets indent_width x nesting characters, so a huge width turns a small statement into gigabytes.
# Out of memory, sqlparse raises MemoryError while Rust stops the process (a failed allocation cannot be
# caught), so widths past any real use stay with sqlparse.
_INDENT_WIDTH_MAX = 4096


def _options_ported(options):
    for name, value in options.items():
        kind = _OPTIONS.get(name)
        t = type(value)
        if kind is _BOOL:
            if t is not bool:
                return False
        elif kind is _OPT_STR:
            if value is not None and t is not str:
                return False
        elif kind is _STR:
            if t is not str:
                return False
        elif kind is _INT or kind is _OPT_INT:
            if kind is _OPT_INT and value is None:
                continue
            if t is not int or not _I64_MIN <= value <= _I64_MAX:
                return False
            if name == 'indent_width' and value > _INDENT_WIDTH_MAX:
                return False
        else:
            return False
    return True


# The default lexer as sqlparse sets it up, taken from a fresh Lexer on first use: the keyword dicts themselves
# (held, so compared by identity), their sizes, and SQL_REGEX by content. A rules list seen to match is
# remembered, so the common call compares one reference.
_default_keywords = None
_default_sizes = None
_default_rx = None
_rx_seen = None


def _regex_content(rx_list):
    return [(m.__self__.pattern, m.__self__.flags, tt) for m, tt in rx_list]


def _lexer_ported():
    global _default_keywords, _default_sizes, _default_rx, _rx_seen
    if _default_keywords is None:
        ref = _lexer.Lexer()
        ref.default_initialization()
        _default_keywords = list(ref._keywords)
        _default_sizes = [len(d) for d in _default_keywords]
        _default_rx = _regex_content(ref._SQL_REGEX)
    lx = _lexer.Lexer.get_default_instance()
    kws = lx._keywords
    if len(kws) != len(_default_keywords):
        return False
    for d, default, size in zip(kws, _default_keywords, _default_sizes):
        if d is not default or len(d) != size:
            return False
    rx = lx._SQL_REGEX
    if rx is _rx_seen:
        return True
    if _regex_content(rx) != _default_rx:
        return False
    _rx_seen = rx
    return True


def format(sql, encoding=None, **options):
    """sqlparse.format(): *sql* formatted according to *options* (see sqlparse's documentation)."""
    if (
        _ported
        and type(sql) is str
        and encoding is None
        and _options_ported(options)
        and _lexer_ported()
    ):
        try:
            # Called from this frame: the Rust counts it in place of sqlparse.format's for the recursion limit.
            return _native.format(sql, **options)
        except UnicodeEncodeError:
            pass
    return _original_format(sql, encoding, **options)


def split(sql, encoding=None, strip_semicolon=False):
    """sqlparse.split(): the statements in *sql*, each stripped."""
    if (
        _ported
        and type(sql) is str
        and encoding is None
        and type(strip_semicolon) is bool
        and _lexer_ported()
    ):
        try:
            return _native.split(sql, strip_semicolon)
        except UnicodeEncodeError:
            pass
    return _original_split(sql, encoding, strip_semicolon)


def install():
    """Make sqlparse.format and sqlparse.split these. Does nothing (returns False) when the installed sqlparse is
    not the version that was ported."""
    if not _ported:
        return False
    sqlparse.format = format
    sqlparse.split = split
    return True


def uninstall():
    """Put sqlparse's own format and split back."""
    sqlparse.format = _original_format
    sqlparse.split = _original_split
