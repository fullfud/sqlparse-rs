"""sqlparse's format() and split(), run in Rust, with sqlparse itself for everything the Rust was not proven on.

    import sqlparse_turbo
    sqlparse_turbo.format(sql, reindent=True, keyword_case='upper')   # same signature and result as sqlparse.format
    sqlparse_turbo.split(sql)

    sqlparse_turbo.split_many(sqls)   # split() of each, over all CPU threads

    sqlparse_turbo.install()   # sqlparse.format and sqlparse.split now go through here, for every caller
                            # that looks them up on the module (sqlparse.format(...)); a name bound earlier
                            # with `from sqlparse import format` keeps the original. sqlparse's default
                            # lexer gets the Rust tokenizer too, so sqlparse.parse() and everything else
                            # that lexes runs it, and FilterStack.run (callers' own filter stacks, as
                            # django-debug-toolbar's) gets its statements grouped in Rust.

The Rust is a port of sqlparse 0.6.1 checked call by call against it (see the project's README). A call goes to
the original sqlparse, unchanged, unless all of this holds:

- the installed sqlparse is 0.6.1 (with any other version every call goes to sqlparse, and install() does
  nothing);
- `sql` is a `str` (not a subclass), or bytes or a text stream that sqlparse's lexer would make one: the stream
  is read and the bytes decoded here exactly as sqlparse does it (`encoding`, else UTF-8, else
  `unicode-escape`), at the point sqlparse does it; `encoding` is not used for a `str`, as in sqlparse;
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

import itertools as _itertools
import unicodedata as _unicodedata
from io import TextIOBase as _TextIOBase
from sys import _getframe, getrecursionlimit as _getrecursionlimit

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

__all__ = ['format', 'split', 'split_many', 'parse', 'install', 'uninstall', 'PORTED_VERSION', 'ACTIVE']

PORTED_VERSION = '0.6.1'

_original_format = sqlparse.format
_original_split = sqlparse.split
_original_parse = sqlparse.parse
_original_get_tokens = _lexer.Lexer.get_tokens
_original_run = sqlparse.engine.filter_stack.FilterStack.run
_ported = sqlparse.__version__ == PORTED_VERSION
# Whether the Rust runs at all: only with the sqlparse version it was ported from. Otherwise every call is
# sqlparse's own, at its own speed.
ACTIVE = _ported
if not _ported:
    import warnings as _warnings

    _warnings.warn(
        f'sqlparse_turbo runs in Rust only with sqlparse {PORTED_VERSION}; sqlparse {sqlparse.__version__} is '
        f'installed, so every call goes to sqlparse at its own speed (pip install "sqlparse=={PORTED_VERSION}")',
        RuntimeWarning,
        stacklevel=2,
    )



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


_drift_ported = _drift_check()

# The port keeps a statement's text positions in 32 bits: texts this long (2 GiB of UTF-8 at most) are sqlparse's.
_MAX_TEXT = 1 << 29


def _text_ported(text):
    """str -> bool: whether the Rust takes the text (its length, its characters)."""
    return len(text) < _MAX_TEXT and _drift_ported(text)


# Raised by the Rust for a statement deep enough, from a caller standing near Python's recursion limit, that a
# grouping pass which does not count frames could reach the limit (see group() in grouping.rs): the statement
# then goes to sqlparse.
_Deep = _native.Deep
# Raised by the Rust when a call's output would grow past what it makes without risk (see INDENT_BUDGET in
# filters.rs): the call then goes to sqlparse, which makes that output itself or raises MemoryError.
_Oversize = _native.Oversize

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
            if value is not None and (t is not str or not _text_ported(value)):
                return None
        elif kind is _STR:
            if t is not str or not _text_ported(value):
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
_default_rx = None
_rx_items = []


def _regex_content(rx_list):
    return [(m.__self__.pattern, m.__self__.flags, tt) for m, tt in rx_list]


def _default_lexer():
    """sqlparse's keyword dicts as its default lexer holds them, and its rules by content."""
    global _default_keywords, _default_rx
    if _default_keywords is None:
        ref = _lexer.Lexer()
        ref.default_initialization()
        _default_keywords = list(ref._keywords)
        _default_rx = _regex_content(ref._SQL_REGEX)
    return _default_keywords


def _lexer_ported():
    """The default lexer as sqlparse sets it up: its keyword dicts the very dicts sqlparse's keywords module
    holds (their content is watched with sqlparse's namespaces), its rules list the very rules found to be
    sqlparse's (a rule replaced, added or removed in place shows), or equal to them by content. The common case,
    the very objects last found, is answered by the Rust in one call."""
    fast = _native.lexer_ported()
    if fast is not None:
        return fast
    global _rx_items
    defaults = _default_lexer()
    lx = _lexer.Lexer.get_default_instance()
    kws = lx._keywords
    if len(kws) != len(defaults):
        return False
    for d, default in zip(kws, defaults):
        if d is not default:
            return False
    rx = lx._SQL_REGEX
    if type(rx) is list and _native.same_items(rx, _rx_items):
        return True
    if _regex_content(rx) != _default_rx:
        return False
    if type(rx) is list:
        _rx_items = list(rx)
        _native.set_lexer_reference(_lexer.Lexer, defaults, _rx_items)
    return True


_RECURSION = 'Maximum recursion depth exceeded'


def _raise_recursion():
    """sqlparse's FilterStack turns a RecursionError into this, `from` it: the same chain here."""
    try:
        raise RecursionError('maximum recursion depth exceeded')
    except RecursionError as err:
        raise _SQLParseError(_RECURSION) from err


# What sqlparse's lexer reads before it lexes: a text stream, or bytes it decodes.
_READ = (_TextIOBase, bytes)


def _text_of(sql, encoding):
    """What sqlparse's Lexer.get_tokens makes of a text stream or bytes before lexing: the same calls in the same
    order, so the same result and the same errors (a read() that fails, a UnicodeDecodeError, a TypeError for a
    stream that reads neither str nor bytes)."""
    if isinstance(sql, _TextIOBase):
        sql = sql.read()
    if isinstance(sql, str):
        return sql
    if isinstance(sql, bytes):
        if encoding:
            return sql.decode(encoding)
        try:
            return sql.decode('utf-8')
        except UnicodeDecodeError:
            return sql.decode('unicode-escape')
    raise TypeError(f"Expected text or file-like object, got {type(sql)!r}")


def _read(sql, encoding):
    """_text_of, with a RecursionError turned into sqlparse's SQLParseError as its FilterStack.run does."""
    try:
        return _text_of(sql, encoding)
    except RecursionError as err:
        raise _SQLParseError(_RECURSION) from err


def format(sql, encoding=None, **options):
    """sqlparse.format(): *sql* formatted according to *options* (see sqlparse's documentation)."""
    try:
        ported = _ported_options(options) if _ported else None
        if ported is not None and type(sql) is not str and isinstance(sql, _READ) and _sqlparse_untouched():
            # sqlparse checks the options first, then its lexer reads the stream or decodes the bytes
            sqlparse.formatter.validate_options(dict(options))
            sql = _read(sql, encoding)
        fast = ported is not None and type(sql) is str and _text_ported(sql) and _sqlparse_untouched()
    except RecursionError:  # a caller at the very limit: the checks cannot run, sqlparse's code below answers
        fast = False
    if fast:
        too_deep = False
        try:
            # Called from this frame: the Rust counts it in place of sqlparse.format's for the recursion limit.
            return _native.format(sql, **ported)
        except (UnicodeEncodeError, _Oversize, _Deep):
            pass
        except _SQLParseError as err:
            if err.args != (_RECURSION,):
                raise
            too_deep = True
        if too_deep:
            _raise_recursion()
    if not _ported:
        return _original_format(sql, encoding, **options)
    # sqlparse 0.6.1's format(), run here rather than called: a call it answers stands where sqlparse's own
    # would, not one frame deeper, so it reaches the recursion limit at the same place
    stack = sqlparse.engine.FilterStack()
    options = sqlparse.formatter.validate_options(options)
    stack = sqlparse.formatter.build_filter_stack(stack, options)
    stack.postprocess.append(sqlparse.filters.SerializerUnicode())
    return ''.join(stack.run(sql, encoding))


def split(sql, encoding=None, strip_semicolon=False):
    """sqlparse.split(): the statements in *sql*, each stripped."""
    try:
        if (_ported and type(sql) is not str and isinstance(sql, _READ) and type(strip_semicolon) is bool
                and _sqlparse_untouched()):
            sql = _read(sql, encoding)
        fast = (_ported and type(sql) is str and type(strip_semicolon) is bool and _text_ported(sql)
                and _sqlparse_untouched())
        if fast:
            # The Rust split takes no Python frames, sqlparse's a few: a caller standing that close to the
            # recursion limit gets sqlparse's code below, which fails where sqlparse's own would.
            try:
                _getframe(_getrecursionlimit() - _NEAR_LIMIT)
            except ValueError:  # the usual case: the stack is not that deep
                pass
            else:
                fast = False
    except RecursionError:  # see format()
        fast = False
    if fast:
        try:
            return _native.split(sql, strip_semicolon)
        except UnicodeEncodeError:
            pass
    if not _ported:
        return _sqlparse_split(sql, encoding, strip_semicolon)
    # sqlparse 0.6.1's split(), run here rather than called (see format())
    stack = sqlparse.engine.FilterStack(strip_semicolon=strip_semicolon)
    return [str(stmt).strip() for stmt in stack.run(sql, encoding)]


# Frames from the recursion limit within which split() leaves the call to sqlparse's code (the margin the Rust
# keeps for format and parse, MARGIN in grouping.rs)
_NEAR_LIMIT = 256


def _sqlparse_split(sql, encoding, strip_semicolon):
    # sqlparse before 0.5 has no strip_semicolon: left out unless asked for, as a direct call would
    if strip_semicolon is False:
        return _original_split(sql, encoding)
    return _original_split(sql, encoding, strip_semicolon=strip_semicolon)


def split_many(sqls, encoding=None, strip_semicolon=False):
    """[split(sql, encoding, strip_semicolon) for sql in sqls], the texts the Rust takes split over all CPU
    threads at once (without Python's lock); the others go to sqlparse one by one."""
    sqls = list(sqls)
    out = [None] * len(sqls)
    if _ported and type(strip_semicolon) is bool and _sqlparse_untouched():
        for i, sql in enumerate(sqls):
            if type(sql) is not str and isinstance(sql, _READ):
                sqls[i] = _read(sql, encoding)
        fast = [i for i, sql in enumerate(sqls) if type(sql) is str and _text_ported(sql)]
        if fast:
            try:
                for i, statements in zip(fast, _native.split_many([sqls[i] for i in fast], strip_semicolon)):
                    out[i] = statements
            except UnicodeEncodeError:
                pass
    for i, statements in enumerate(out):
        if statements is None:
            out[i] = _sqlparse_split(sqls[i], encoding, strip_semicolon)
    return out


def _token_type(name):
    t = _tokens.Token
    for part in name.split('.')[1:]:
        t = getattr(t, part)
    return t


if _ported:
    _native.set_token_types({name: _token_type(name) for name in _native.token_type_names()})
    _native.set_sql_objects(
        {name: cls for name, cls in vars(_sql).items() if isinstance(cls, type) and issubclass(cls, _sql.Token)},
        object.__new__,
    )


def parse(sql, encoding=None):
    """sqlparse.parse(): the statements in *sql*, grouped, as sqlparse's own objects (a tuple of
    `sqlparse.sql.Statement`). Grouped in Rust and built node for node as sqlparse leaves them; in sqlparse
    under the same conditions as format()."""
    try:
        if _ported and type(sql) is not str and isinstance(sql, _READ) and _sqlparse_untouched():
            sql = _read(sql, encoding)
        fast = _ported and type(sql) is str and _text_ported(sql) and _sqlparse_untouched()
    except RecursionError:  # see format()
        fast = False
    if fast:
        too_deep = False
        try:
            # Called from this frame: the Rust counts it in place of sqlparse.parse's for the recursion limit.
            return _native.parse(sql)
        except (UnicodeEncodeError, _Deep):
            pass
        except _SQLParseError as err:
            if err.args != (_RECURSION,):
                raise
            too_deep = True
        if too_deep:
            _raise_recursion()
    if not _ported:
        return _original_parse(sql, encoding)
    # sqlparse 0.6.1's parse(), run here rather than called (see format())
    return tuple(sqlparse.parsestream(sql, encoding))


def _get_tokens(self, text, encoding=None):
    """sqlparse's Lexer.get_tokens, with install(): sqlparse's default lexer, as sqlparse sets it up, on a `str`
    the Rust takes, tokenizes in Rust; anything else (another Lexer, a changed one, bytes, a stream) goes to
    sqlparse's own. Same (token type, value) pairs, same token type objects. A stream is read and bytes decoded
    as sqlparse's would, when the first token is asked for."""
    if (self is _lexer.Lexer._default_instance and type(text) is not str and isinstance(text, _READ)
            and _sqlparse_untouched()):
        text = _text_of(text, encoding)
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


def _first_filter(stmtprocess):
    """The stack's first statement filter and its state, when the Rust can run it in its place: sqlparse's own
    AlignedIndentFilter or ReindentFilter, nothing on the instance but what __init__ sets, every value of the type
    and within the size the port takes (larger ones make Python strings the port does not risk). Returns (filter,
    AlignedIndentFilter state, ReindentFilter state), one of the states None; _NO_FIRST when the Rust cannot.

    A ReindentFilter's state includes str(_last_stmt).endswith('\\n'), which its process() takes after it has
    processed the statement: taken here, from the frame where process() would run, before (nothing the filter does
    to this statement changes the last one), so a RecursionError comes from the same depth; it then goes to the
    Python filter."""
    if type(stmtprocess) is not list or not stmtprocess:
        return _NO_FIRST
    f = stmtprocess[0]
    kind = type(f)
    if kind is not _AlignedIndentFilter and kind is not _ReindentFilter:
        return _NO_FIRST
    d = getattr(f, '__dict__', None)
    if type(d) is not dict:
        return _NO_FIRST
    if kind is _AlignedIndentFilter:
        if d.keys() != _ALIGNED_KEYS:
            return _NO_FIRST
        n, ch, off, ind, kwd = d['n'], d['char'], d['offset'], d['indent'], d['_max_kwd_len']
        if (type(n) is str and type(ch) is str and type(off) is int and type(ind) is int and type(kwd) is int
                and len(n) <= 64 and len(ch) <= 16 and -100 <= off <= 100 and -100 <= ind <= 100
                and 0 <= kwd <= 100 and (n.isascii() or _utf8(n)) and (ch.isascii() or _utf8(ch))):
            return f, (n, ch, off, ind, kwd), None
        return _NO_FIRST
    if d.keys() != _REINDENT_KEYS:
        return _NO_FIRST
    n, ch, width, ind, off, wrap = d['n'], d['char'], d['width'], d['indent'], d['offset'], d['wrap_after']
    comma_first, indent_columns, compact = d['comma_first'], d['indent_columns'], d['compact']
    last_stmt, last_func = d['_last_stmt'], d['_last_func']
    if not (type(n) is str and type(ch) is str and type(width) is int and type(ind) is int and type(off) is int
            and type(wrap) is int and type(comma_first) is bool and type(indent_columns) is bool
            and type(compact) is bool and len(n) <= 64 and len(ch) <= 16 and -4096 <= width <= 4096
            and -64 <= ind <= 64 and -4096 <= off <= 4096 and -_WRAP_RANGE <= wrap <= _WRAP_RANGE
            and (n.isascii() or _utf8(n)) and (ch.isascii() or _utf8(ch))):
        return _NO_FIRST
    if last_func is None:
        func_len = None
    elif type(last_func) in _SQL_CLASSES and type(last_func.value) is str:
        func_len = len(last_func.value)
    else:
        return _NO_FIRST
    if last_stmt is None:
        ends_nl = None
    elif type(last_stmt) is _sql.Statement:
        ends_nl = str(last_stmt).endswith('\n')
    else:
        return _NO_FIRST
    return f, None, (n, ch, width, ind, off, wrap, comma_first, indent_columns, compact, func_len, ends_nl)


def _utf8(text):
    try:
        text.encode('utf-8')
    except UnicodeEncodeError:  # lone surrogates
        return False
    return True


_AlignedIndentFilter = sqlparse.filters.aligned_indent.AlignedIndentFilter
_ALIGNED_KEYS = frozenset(vars(_AlignedIndentFilter()))
_ReindentFilter = sqlparse.filters.reindent.ReindentFilter
_REINDENT_KEYS = frozenset(vars(_ReindentFilter()))
_WRAP_RANGE = 1 << 62
_SQL_CLASSES = frozenset(c for c in vars(_sql).values() if isinstance(c, type) and issubclass(c, _sql.Token))
_NO_FIRST = (None, None, None)
_filter_stack = sqlparse.engine.filter_stack


def _run(self, sql, encoding=None):
    """sqlparse's FilterStack.run, with install(). A generator, as sqlparse's: calling it does nothing, and its frame
    stands where sqlparse's stands. When the first statement is asked for, a stack with grouping on and no
    token-stream filters on a text the Rust takes gets its statements grouped in Rust, one at a time as
    sqlparse's run does (the objects parse() returns, as sqlparse's run has them at that point; a leading
    AlignedIndentFilter or ReindentFilter of sqlparse's run in Rust as well, the filter's instance left as its
    process() leaves it), and runs the rest of its statement filters and its serializers on them in Python.
    Anything else runs sqlparse's own run, here in this frame."""
    statements = None
    try:
        if self._grouping and not self.preprocess and _sqlparse_untouched():
            if type(sql) is not str and isinstance(sql, _READ):
                sql = _read(sql, encoding)
            if type(sql) is str and _text_ported(sql):
                statements = _native.statements(sql)
    except (UnicodeEncodeError, RecursionError):  # RecursionError: the checks, from a caller at the very limit
        statements = None
    if statements is None:
        # sqlparse 0.6.1's FilterStack.run, run here rather than called (its frames, not one more)
        try:
            stream = _filter_stack.lexer.tokenize(sql, encoding)
            for filter_ in self.preprocess:
                stream = filter_.process(stream)
            stream = _filter_stack.StatementSplitter().process(stream)
            for stmt in stream:
                if self._grouping:
                    stmt = _filter_stack.grouping.group(stmt)
                for filter_ in self.stmtprocess:
                    filter_.process(stmt)
                for filter_ in self.postprocess:
                    stmt = filter_.process(stmt)
                yield stmt
        except RecursionError as err:
            raise _filter_stack.SQLParseError(_RECURSION) from err
        return
    while True:
        if not self._grouping:
            # grouping turned off since the first statement (sqlparse's run looks at it for each one): the next
            # statement as sqlparse's splitter makes it, filtered from this frame
            try:
                if not statements.skip():
                    return
                stmt = _sql.Statement([_sql.Token(ttype, value) for ttype, value in statements.last_tokens()])
                for filter_ in self.stmtprocess:
                    filter_.process(stmt)
                for filter_ in self.postprocess:
                    stmt = filter_.process(stmt)
            except RecursionError as err:
                raise _filter_stack.SQLParseError(_RECURSION) from err
            yield stmt
            continue
        too_deep = deep = False
        try:
            first, aligned, reindent = _first_filter(self.stmtprocess)
        except RecursionError:  # at the very limit: the Python filter runs instead
            first = aligned = reindent = None
        try:
            # Called from this generator, which stands where sqlparse's run stands: the Rust counts it so.
            got = statements.next(aligned, reindent)
        except _Deep:
            deep = True
        except RecursionError as err:
            # a Python call the Rust makes failed at the very limit: so would sqlparse's splitting and grouping
            # of the next statement, inside its run
            raise _filter_stack.SQLParseError(_RECURSION) from err
        except _SQLParseError as err:
            if err.args != (_RECURSION,):
                raise
            too_deep = True
        if too_deep:
            # sqlparse's run turns the RecursionError into this, `from` it (raised here, not in a helper's frame)
            try:
                raise RecursionError('maximum recursion depth exceeded')
            except RecursionError as err:
                raise _filter_stack.SQLParseError(_RECURSION) from err
        if deep:
            # this statement as sqlparse's splitter makes it, grouped and filtered by sqlparse from this frame
            try:
                stmt = _sql.Statement([_sql.Token(ttype, value) for ttype, value in statements.last_tokens()])
                stmt = _filter_stack.grouping.group(stmt)
                for filter_ in self.stmtprocess:
                    filter_.process(stmt)
                for filter_ in self.postprocess:
                    stmt = filter_.process(stmt)
            except RecursionError as err:
                raise _filter_stack.SQLParseError(_RECURSION) from err
            yield stmt
            continue
        if got is None:
            return
        stmt, first_done, last_func = got
        if first_done and reindent is not None:
            # what ReindentFilter.process leaves on the instance
            first._curr_stmt = stmt
            if last_func is not None:
                first._last_func = last_func
            first._last_stmt = stmt
        try:
            # the stack's own list, as sqlparse iterates it, past the first filter if the Rust ran it
            for filter_ in _itertools.islice(self.stmtprocess, 1, None) if first_done else self.stmtprocess:
                filter_.process(stmt)
            for filter_ in self.postprocess:
                stmt = filter_.process(stmt)
        except RecursionError as err:
            raise _filter_stack.SQLParseError(_RECURSION) from err
        yield stmt


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
    (sqlparse.engine.filter_stack.FilterStack, 'run'): _run,
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
    # the default lexer's keyword dicts: every keyword with the very token type it had (a keyword retyped,
    # added or removed in place shows)
    for kwdict in _default_lexer():
        watch.append((kwdict, True, [(name, [value], False) for name, value in kwdict.items()]))
    _native.set_watch(watch)
    return [namespace for namespace, _, _ in watch]


_watched_dicts = _watch() if _ported else []

# Comparing every watched name costs a few microseconds a call. CPython 3.12+ can tell us instead when one of
# these dicts changes (a dict watcher): the comparison then runs only after a change, not on every call. The
# watcher's callback is the Rust's own C function, which counts the change and runs no Python code (so no other
# thread can slip in between the count and the change, and nothing runs Python at shutdown). On older Pythons,
# or when no watcher can be had, the comparison runs on every call.
def _add_watchers(dicts):
    import sys

    if sys.version_info < (3, 12) or sys.implementation.name != 'cpython':
        return False
    try:
        import ctypes

        api = ctypes.pythonapi
        api.PyDict_AddWatcher.argtypes = [ctypes.c_void_p]
        api.PyDict_AddWatcher.restype = ctypes.c_int
        api.PyDict_Watch.argtypes = [ctypes.c_int, ctypes.py_object]
        api.PyDict_Watch.restype = ctypes.c_int
        watcher = _native.watcher()  # taken by an earlier import of this package (a reload)
        if watcher is None:
            watcher = api.PyDict_AddWatcher(_native.watch_callback())
            if watcher < 0:
                return False
            _native.set_watcher(watcher)
        for d in dicts:
            if api.PyDict_Watch(watcher, d) < 0:
                return False
    except Exception:  # no ctypes, or no watcher to be had (RuntimeError when the interpreter's slots are taken)
        return False
    return True


_watching = _add_watchers(_watched_dicts) if _ported else False


def _sqlparse_untouched():
    """sqlparse as the Rust reproduces it: nothing replaced, and its default lexer as sqlparse sets it up."""
    if not _watching:
        return _native.untouched() and _lexer_ported()
    return _native.untouched_watched() and _lexer_ported()


def install():
    """Make sqlparse.format and sqlparse.split these, and give sqlparse's default lexer the Rust tokenizer. Does
    nothing (returns False) when the installed sqlparse is not the version that was ported."""
    if not _ported:
        return False
    sqlparse.format = format
    sqlparse.split = split
    sqlparse.parse = parse
    _lexer.Lexer.get_tokens = _get_tokens
    sqlparse.engine.filter_stack.FilterStack.run = _run
    return True


def uninstall():
    """Put sqlparse's own format, split and lexer back."""
    sqlparse.format = _original_format
    sqlparse.split = _original_split
    sqlparse.parse = _original_parse
    _lexer.Lexer.get_tokens = _original_get_tokens
    sqlparse.engine.filter_stack.FilterStack.run = _original_run
