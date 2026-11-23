"""sqlparse's format() and split(), run in Rust, with sqlparse itself for everything the Rust was not proven on.

    import sqlparse_rs
    sqlparse_rs.format(sql, reindent=True, keyword_case='upper')   # same signature and result as sqlparse.format
    sqlparse_rs.split(sql)

    sqlparse_rs.split_many(sqls)   # split() of each, over all CPU threads

    sqlparse_rs.install()   # sqlparse.format and sqlparse.split now go through here, for every caller
                            # that looks them up on the module (sqlparse.format(...)); a name bound earlier
                            # with `from sqlparse import format` keeps the original. sqlparse's default
                            # lexer gets the Rust tokenizer too, so sqlparse.parse() and everything else
                            # that lexes runs it.

The Rust is a port of sqlparse 0.6.1 checked call by call against it (see the project's README). A call goes to
the original sqlparse, unchanged, unless all of this holds:

- the installed sqlparse is 0.6.1;
- `sql` is a `str` (not bytes, not a stream, not a subclass); `encoding` is then not used, as in sqlparse,
  which only decodes bytes with it;
- sqlparse's default lexer is as sqlparse sets it up (no add_keywords, set_SQL_REGEX, clear), and nothing in
  sqlparse's modules or classes was replaced, added or removed since this package was imported (a patched
  grouping function, filter or method goes to sqlparse, which uses it);
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
message, and so does a statement nested past Python's recursion limit, at the same depth as in sqlparse and,
as there, raised from a RecursionError.
"""

import unicodedata as _unicodedata

import sqlparse
import sqlparse.engine.filter_stack
import sqlparse.engine.grouping
import sqlparse.engine.statement_splitter
import sqlparse.filters.aligned_indent
import sqlparse.filters.others
import sqlparse.filters.output
import sqlparse.filters.reindent
import sqlparse.filters.right_margin
import sqlparse.filters.tokens
import sqlparse.formatter
import sqlparse.utils
from sqlparse import lexer as _lexer
from sqlparse import sql as _sql
from sqlparse.exceptions import SQLParseError as _SQLParseError
from sqlparse import tokens as _tokens

from . import _native
from ._unicode_drift import DRIFT as _DRIFT
from ._unicode_drift import PORTED_UNICODE as _PORTED_UNICODE

__all__ = ['format', 'split', 'split_many', 'parse', 'install', 'uninstall', 'PORTED_VERSION']

PORTED_VERSION = '0.6.1'

_original_format = sqlparse.format
_original_split = sqlparse.split
_original_parse = sqlparse.parse
_original_get_tokens = _lexer.Lexer.get_tokens
_ported = sqlparse.__version__ == PORTED_VERSION



def _drift_check():
    """str -> bool: whether the Rust treats every character of the text as this Python does. The drift list goes
    to the Rust once (a bit per code point); a text is then checked in one pass over it."""
    version = _unicodedata.unidata_version
    if version == _PORTED_UNICODE:
        return lambda text: True
    # a Unicode version the list does not know: no non-ASCII text runs in Rust
    _native.set_drift(list(_DRIFT.get(version, ((0x80, 0x10FFFF),))))
    drifted = _native.drifted

    def ported(text):
        if text.isascii():
            return True
        try:
            return not drifted(text)
        except UnicodeEncodeError:  # lone surrogates: sqlparse's
            return False

    return ported


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


_RECURSION = 'Maximum recursion depth exceeded'


def _raise_recursion():
    """sqlparse's FilterStack turns a RecursionError into this, `from` it: the same chain here."""
    try:
        raise RecursionError('maximum recursion depth exceeded')
    except RecursionError as err:
        raise _SQLParseError(_RECURSION) from err


def format(sql, encoding=None, **options):
    """sqlparse.format(): *sql* formatted according to *options* (see sqlparse's documentation)."""
    ported = _ported_options(options) if _ported and type(sql) is str else None
    if ported is not None and _text_ported(sql) and _sqlparse_untouched():
        too_deep = False
        try:
            # Called from this frame: the Rust counts it in place of sqlparse.format's for the recursion limit.
            return _native.format(sql, **ported)
        except UnicodeEncodeError:
            pass
        except _SQLParseError as err:
            if err.args != (_RECURSION,):
                raise
            too_deep = True
        if too_deep:
            _raise_recursion()
    return _original_format(sql, encoding, **options)


def split(sql, encoding=None, strip_semicolon=False):
    """sqlparse.split(): the statements in *sql*, each stripped."""
    if (
        _ported
        and type(sql) is str
        and type(strip_semicolon) is bool
        and _text_ported(sql)
        and _sqlparse_untouched()
    ):
        try:
            return _native.split(sql, strip_semicolon)
        except UnicodeEncodeError:
            pass
    return _original_split(sql, encoding, strip_semicolon)


def split_many(sqls, encoding=None, strip_semicolon=False):
    """[split(sql, encoding, strip_semicolon) for sql in sqls], the texts the Rust takes split over all CPU
    threads at once (without Python's lock); the others go to sqlparse one by one."""
    sqls = list(sqls)
    out = [None] * len(sqls)
    if _ported and type(strip_semicolon) is bool and _sqlparse_untouched():
        fast = [i for i, sql in enumerate(sqls) if type(sql) is str and _text_ported(sql)]
        if fast:
            try:
                for i, statements in zip(fast, _native.split_many([sqls[i] for i in fast], strip_semicolon)):
                    out[i] = statements
            except UnicodeEncodeError:
                pass
    for i, statements in enumerate(out):
        if statements is None:
            out[i] = _original_split(sqls[i], encoding, strip_semicolon)
    return out


def _token_type(name):
    t = _tokens.Token
    for part in name.split('.')[1:]:
        t = getattr(t, part)
    return t


_native.set_token_types({name: _token_type(name) for name in _native.token_type_names()})
_native.set_sql_objects(
    {name: cls for name, cls in vars(_sql).items() if isinstance(cls, type) and issubclass(cls, _sql.Token)},
    object.__new__,
)


def parse(sql, encoding=None):
    """sqlparse.parse(): the statements in *sql*, grouped, as sqlparse's own objects (a tuple of
    `sqlparse.sql.Statement`). Grouped in Rust and built node for node as sqlparse leaves them; in sqlparse
    under the same conditions as format()."""
    if _ported and type(sql) is str and _text_ported(sql) and _sqlparse_untouched():
        too_deep = False
        try:
            # Called from this frame: the Rust counts it in place of sqlparse.parse's for the recursion limit.
            return _native.parse(sql)
        except UnicodeEncodeError:
            pass
        except _SQLParseError as err:
            if err.args != (_RECURSION,):
                raise
            too_deep = True
        if too_deep:
            _raise_recursion()
    return _original_parse(sql, encoding)


def _get_tokens(self, text, encoding=None):
    """sqlparse's Lexer.get_tokens, with install(): sqlparse's default lexer, as sqlparse sets it up, on a `str`
    the Rust takes, tokenizes in Rust; anything else (another Lexer, a changed one, bytes, a stream) goes to
    sqlparse's own. Same (token type, value) pairs, same token type objects."""
    if (self is _lexer.Lexer._default_instance and type(text) is str and _text_ported(text)
            and _sqlparse_untouched()):
        try:
            tokens = _native.lex(text)
        except UnicodeEncodeError:
            tokens = None
        if tokens is not None:
            yield from tokens
            return
    yield from _original_get_tokens(self, text, encoding)


# Everything of sqlparse the Rust reproduces: its modules' names and its classes' attributes, as imported. Where
# install() puts the package's own function, that one is allowed as well.
_WATCHED_MODULES = [
    sqlparse, _sql, _tokens, sqlparse.keywords, _lexer, sqlparse.utils, sqlparse.formatter, sqlparse.engine,
    sqlparse.engine.grouping, sqlparse.engine.filter_stack, sqlparse.engine.statement_splitter, sqlparse.filters,
    sqlparse.filters.aligned_indent, sqlparse.filters.others, sqlparse.filters.output, sqlparse.filters.reindent,
    sqlparse.filters.right_margin, sqlparse.filters.tokens,
]
_INSTALLED = {
    (sqlparse, 'format'): format, (sqlparse, 'split'): split, (sqlparse, 'parse'): parse,
    (_lexer.Lexer, 'get_tokens'): _get_tokens,
}


def _watch():
    import gc

    owners = list(_WATCHED_MODULES)
    for module in _WATCHED_MODULES:
        for value in list(vars(module).values()):
            if isinstance(value, type) and value.__module__.startswith('sqlparse') and value not in owners:
                owners.append(value)
    watch = []
    for owner in owners:
        namespace = vars(owner)
        if type(namespace) is not dict:  # a class's mappingproxy: the dict behind it
            namespace = next(r for r in gc.get_referents(namespace) if type(r) is dict)
        entries = []
        for name, value in namespace.items():
            if (owner, name) == (_lexer.Lexer, '_default_instance'):  # set on first use
                allowed = None
            elif (owner, name) in _INSTALLED:
                allowed = [value, _INSTALLED[owner, name]]
            else:
                allowed = [value]
            entries.append((name, allowed, type(value) in (int, float, str, bytes, bool, tuple, frozenset)))
        watch.append((namespace, isinstance(owner, type), entries))
    _native.set_watch(watch)
    return [namespace for namespace, _, _ in watch]


_watched_dicts = _watch()

# Comparing every watched name costs a few microseconds a call. CPython 3.12+ can tell us instead when one of
# these dicts changes (a dict watcher): the comparison then runs once after a change, not on every call. On
# older Pythons, or when no watcher can be had, it runs on every call.
_changed = [True]  # a change not yet compared; True until the first comparison


def _add_watchers(dicts):
    import sys

    if sys.version_info < (3, 12) or sys.implementation.name != 'cpython':
        return False
    try:
        import atexit
        import ctypes

        api = ctypes.pythonapi
        callback_type = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p,
                                         ctypes.c_void_p)
        changed = _changed

        def on_change(event, d, key, value):
            changed[0] = True
            return 0

        callback = callback_type(on_change)
        api.PyDict_AddWatcher.argtypes = [callback_type]
        api.PyDict_AddWatcher.restype = ctypes.c_int
        api.PyDict_Watch.argtypes = [ctypes.c_int, ctypes.py_object]
        api.PyDict_Watch.restype = ctypes.c_int
        api.PyDict_ClearWatcher.argtypes = [ctypes.c_int]
        api.PyDict_ClearWatcher.restype = ctypes.c_int
        watcher = api.PyDict_AddWatcher(callback)
        if watcher < 0:  # all of the interpreter's watcher slots are taken
            return False
        for d in dicts:
            if api.PyDict_Watch(watcher, d) < 0:
                api.PyDict_ClearWatcher(watcher)
                return False
    except (AttributeError, OSError, ValueError, TypeError, ctypes.ArgumentError):
        return False

    def stop():
        # before the interpreter tears modules down, which would call back into Python mid-shutdown
        api.PyDict_ClearWatcher(watcher)
        _changed[0] = True

    atexit.register(stop)
    _add_watchers.callback = callback  # the C function pointer lives as long as this object
    return True


_watching = _add_watchers(_watched_dicts)


def _sqlparse_untouched():
    """sqlparse as the Rust reproduces it: nothing replaced, and its default lexer as sqlparse sets it up."""
    if not _watching:
        return _native.untouched() and _lexer_ported()
    if _changed[0]:
        _changed[0] = False  # set again by a change that lands from here on
        if not _native.untouched():
            _changed[0] = True  # still different: compared again next call, in case it is put back
            return False
    return _lexer_ported()


def install():
    """Make sqlparse.format and sqlparse.split these, and give sqlparse's default lexer the Rust tokenizer. Does
    nothing (returns False) when the installed sqlparse is not the version that was ported."""
    if not _ported:
        return False
    sqlparse.format = format
    sqlparse.split = split
    sqlparse.parse = parse
    _lexer.Lexer.get_tokens = _get_tokens
    return True


def uninstall():
    """Put sqlparse's own format, split and lexer back."""
    sqlparse.format = _original_format
    sqlparse.split = _original_split
    sqlparse.parse = _original_parse
    _lexer.Lexer.get_tokens = _original_get_tokens
