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
- `sql` is a `str` (not bytes, not a stream, not a subclass); `encoding` is then not used, as in sqlparse,
  which only decodes bytes with it;
- sqlparse's default lexer is as sqlparse sets it up (no add_keywords, set_SQL_REGEX, clear);
- every format() option has exactly the type the port takes: bool options are `bool` (not 0/1 or None),
  `indent_width`/`wrap_after` are `int` (not bool) and `truncate_strings`/`right_margin` `int` or None, within
  64 bits (`indent_width` at most 4096, see _INDENT_WIDTH_MAX), `keyword_case`/`identifier_case`/`output_format`
  `str` or None, `truncate_char` `str`; other keywords are dropped, as sqlparse never reads them (`indent_char`,
  which it does read, it always overwrites from `indent_tabs`); split's `strip_semicolon` is a `bool`;
- the text can be encoded as UTF-8 (a str with lone surrogates cannot);
- the text holds no code point this Python's Unicode version treats differently from the Unicode 14 the Rust
  was generated from (CPython 3.11): see _unicode_drift.py. ASCII text always qualifies; on a Unicode version
  not listed there, only ASCII text does.

Errors are sqlparse's own: an invalid option value raises `sqlparse.exceptions.SQLParseError` with sqlparse's
message, and so does a statement nested past Python's recursion limit, at the same depth as in sqlparse.
"""

import re as _re
import unicodedata as _unicodedata

import sqlparse
from sqlparse import lexer as _lexer

from . import _native
from ._unicode_drift import DRIFT as _DRIFT
from ._unicode_drift import PORTED_UNICODE as _PORTED_UNICODE

__all__ = ['format', 'split', 'install', 'uninstall', 'PORTED_VERSION']

PORTED_VERSION = '0.6.1'

_original_format = sqlparse.format
_original_split = sqlparse.split
_ported = sqlparse.__version__ == PORTED_VERSION



def _drift_check():
    """str -> bool: whether the Rust treats every character of the text as this Python does."""
    version = _unicodedata.unidata_version
    if version == _PORTED_UNICODE:
        return lambda text: True
    if version not in _DRIFT:
        return str.isascii
    found = _re.compile('[' + ''.join(f'\\U{a:08x}-\\U{b:08x}' for a, b in _DRIFT[version]) + ']').search
    return lambda text: text.isascii() or not found(text)


_text_ported = _drift_check()

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


def _ported_options(options):
    """The options to give the Rust, or None when a call has to go to sqlparse. Keywords sqlparse does not read
    are left out."""
    unknown = False
    for name, value in options.items():
        kind = _OPTIONS.get(name)
        if kind is None:
            unknown = True
            continue
        t = type(value)
        if kind is _BOOL:
            if t is not bool:
                return None
        elif kind is _OPT_STR:
            if value is not None and t is not str:
                return None
        elif kind is _STR:
            if t is not str:
                return None
        elif kind is _INT or kind is _OPT_INT:
            if kind is _OPT_INT and value is None:
                continue
            if t is not int or not _I64_MIN <= value <= _I64_MAX:
                return None
            if name == 'indent_width' and value > _INDENT_WIDTH_MAX:
                return None
    if unknown:
        return {name: value for name, value in options.items() if name in _OPTIONS}
    return options


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
    ported = _ported_options(options) if _ported and type(sql) is str else None
    if ported is not None and _text_ported(sql) and _lexer_ported():
        try:
            # Called from this frame: the Rust counts it in place of sqlparse.format's for the recursion limit.
            return _native.format(sql, **ported)
        except UnicodeEncodeError:
            pass
    return _original_format(sql, encoding, **options)


def split(sql, encoding=None, strip_semicolon=False):
    """sqlparse.split(): the statements in *sql*, each stripped."""
    if (
        _ported
        and type(sql) is str
        and type(strip_semicolon) is bool
        and _text_ported(sql)
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
