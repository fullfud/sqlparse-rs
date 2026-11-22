"""The package against sqlparse, through the public calls: sqlparse.format/split against sqlparse_rs.format/split.

    python -X utf8 tools/check_package.py

1. The benchmark corpus through the option sets people use, and split(): same result or same exception (class
   and message), with every call run in Rust.
2. Calls outside what was ported: each must give sqlparse's own answer and must not reach the Rust.
3. A changed default lexer goes to sqlparse; put back with default_initialization(), calls go to Rust again.
"""
import glob
import os
import sys
import time

import sqlparse
import sqlparse_rs
from sqlparse import keywords, tokens
from sqlparse.lexer import Lexer

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NATIVE = {sqlparse_rs._native.format: 'format', sqlparse_rs._native.split: 'split'}
counts = {'format': 0, 'split': 0}


def hook(frame, event, arg):
    if event == 'c_call' and arg in NATIVE:
        counts[NATIVE[arg]] += 1


def native_calls():
    return counts['format'] + counts['split']


def rearm():
    """Python drops a profile hook that raises: near the recursion limit calling the hook itself fails. True when
    that happened (the call just made was not counted)."""
    if sys.getprofile() is None:
        sys.setprofile(hook)
        return True
    return False


def outcome(f, *args, **kwargs):
    try:
        return ('ok', f(*args, **kwargs))
    except Exception as e:
        return ('err', f'{type(e).__module__}.{type(e).__qualname__}', str(e))


bad = 0


def same(what, a, b):
    global bad
    if a != b:
        bad += 1
        if bad <= 10:
            print('MISMATCH', what, '\n  sqlparse   ', repr(a)[:300], '\n  sqlparse_rs', repr(b)[:300])


OPTION_SETS = [
    {},
    {'reindent': True},
    {'reindent': True, 'keyword_case': 'upper'},
    {'reindent': True, 'keyword_case': 'upper', 'identifier_case': 'lower', 'strip_comments': True},
    {'reindent_aligned': True},
    {'reindent': True, 'indent_columns': True, 'indent_width': 4},
    {'reindent': True, 'comma_first': True, 'wrap_after': 40},
    {'reindent': True, 'compact': True, 'indent_tabs': True},
    {'reindent': True, 'indent_after_first': True, 'use_space_around_operators': True},
    {'strip_comments': True, 'strip_whitespace': True},
    {'keyword_case': 'capitalize', 'truncate_strings': 8, 'truncate_char': '…'},
    {'reindent': True, 'output_format': 'python'},
    {'output_format': 'php', 'keyword_case': 'lower'},
]


def corpus():
    texts = [open(p, encoding='utf-8').read() for p in sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', 'q*.sql')))]
    texts += ['', ';', 'select 1; select 2;', "select 'unclosed", 'BEGIN ' * 990 + 'END ' * 990,
              'select a ' + 'between ' * 980]
    before = native_calls()
    n = uncounted = 0
    t_py = t_rs = 0.0
    for opts in OPTION_SETS:
        for t in texts:
            t0 = time.perf_counter()
            a = outcome(sqlparse.format, t, **opts)
            t1 = time.perf_counter()
            rearm()
            b = outcome(sqlparse_rs.format, t, **opts)
            t2 = time.perf_counter()
            uncounted += rearm()
            t_py += t1 - t0
            t_rs += t2 - t1
            same(f'format {opts} {t[:60]!r}', a, b)
            n += 1
    for t in texts:
        for strip in (False, True):
            a = outcome(sqlparse.split, t, strip_semicolon=strip)
            rearm()
            same(f'split {strip} {t[:60]!r}', a, outcome(sqlparse_rs.split, t, strip_semicolon=strip))
            uncounted += rearm()
            n += 1
    ran = native_calls() - before
    print(f'corpus: {n} calls compared, {ran} counted in Rust, {uncounted} not countable (the hook hit the '
          f'recursion limit); format: sqlparse {t_py:.1f} s, sqlparse_rs {t_rs:.1f} s')
    if ran > n or ran + uncounted < n:
        print('NOT ALL IN RUST')
        global bad
        bad += 1


class Sub(str):
    pass


def fallbacks():
    sql = "select a, b from t where x = 1 -- note\n; select 'two'"
    cases = [
        ('reindent=None', (sql,), {'reindent': None}),
        ('reindent=1', (sql,), {'reindent': 1}),
        ('reindent=0', (sql,), {'reindent': 0}),
        ('indent_width=True', (sql,), {'reindent': True, 'indent_width': True}),
        ('indent_width=4.0', (sql,), {'reindent': True, 'indent_width': 4.0}),
        ('indent_width=10**6', (sql,), {'reindent': True, 'indent_width': 10**6}),
        ('indent_width=2**70', (sql,), {'reindent': True, 'indent_width': 2**70}),
        ('truncate_strings="3"', (sql,), {'truncate_strings': '3'}),
        ('keyword_case=Sub', (sql,), {'keyword_case': Sub('upper')}),
        ('unknown option', (sql,), {'no_such_option': 1}),
        ('bytes', (sql.encode(),), {}),
        ('bytes + encoding', (sql.encode('utf-16'), 'utf-16'), {}),
        ('str + encoding', (sql, 'utf-8'), {}),
        ('str subclass', (Sub(sql),), {'reindent': True}),
        # The two below do reach the Rust: the text cannot become UTF-8, the call raises UnicodeEncodeError before
        # any work, and the package hands it to sqlparse.
        ('lone surrogate', ('select \ud800 from t',), {'keyword_case': 'upper'}),
        ('lone surrogate in option', (sql,), {'truncate_strings': 3, 'truncate_char': '\udc80'}),
        ('stream', (__import__('io').StringIO(sql),), {}),
    ]
    for name, args, opts in cases:
        rearm()
        before = native_calls()
        b = outcome(sqlparse_rs.format, *args, **opts)
        ran = native_calls() - before
        if hasattr(args[0], 'seek'):
            args[0].seek(0)
        a = outcome(sqlparse.format, *args, **opts)
        same(f'format fallback {name}', a, b)
        if ran and not name.startswith('lone surrogate'):
            print('REACHED RUST', name)
            global bad
            bad += 1
    for name, args, kwargs in [('strip_semicolon=1', (sql,), {'strip_semicolon': 1}),
                               ('strip_semicolon=None', (sql,), {'strip_semicolon': None}),
                               ('bytes', (sql.encode(),), {}), ('encoding', (sql, 'utf-8'), {})]:
        before = native_calls()
        b = outcome(sqlparse_rs.split, *args, **kwargs)
        ran = native_calls() - before
        same(f'split fallback {name}', outcome(sqlparse.split, *args, **kwargs), b)
        if ran:
            print('REACHED RUST', name)
            bad += 1
    print(f'fallbacks: {len(cases) + 4} calls, all answered by sqlparse' if not bad else 'fallbacks: see above')


def lexer_changes():
    global bad
    sql = 'select foo, bar from t where baz is null'
    lx = Lexer.get_default_instance()
    changes = [
        ('add_keywords', lambda: lx.add_keywords({'FOO': tokens.Keyword})),
        ('set_SQL_REGEX', lambda: lx.set_SQL_REGEX([(r'\s+', tokens.Whitespace), (r'\w+', tokens.Name)] + keywords.SQL_REGEX)),
        ('clear + same rules', lambda: (lx.clear(), lx.set_SQL_REGEX(keywords.SQL_REGEX[1:]),
                                        lx.add_keywords(keywords.KEYWORDS_COMMON))),
    ]
    for name, change in changes:
        rearm()
        change()
        for f_py, f_rs, kw in ((sqlparse.format, sqlparse_rs.format, {'reindent': True, 'keyword_case': 'upper'}),
                               (sqlparse.split, sqlparse_rs.split, {})):
            before = native_calls()
            b = outcome(f_rs, sql, **kw)
            ran = native_calls() - before
            same(f'lexer {name}', outcome(f_py, sql, **kw), b)
            if ran:
                print('REACHED RUST with lexer change', name)
                bad += 1
        lx.default_initialization()
        before = native_calls()
        same(f'lexer restored after {name}', outcome(sqlparse.format, sql, reindent=True),
             outcome(sqlparse_rs.format, sql, reindent=True))
        if native_calls() - before != 1:
            print('NOT IN RUST after default_initialization', name)
            bad += 1
    print('lexer changes: done')


def installed():
    global bad
    rearm()
    assert sqlparse_rs.install()
    from sqlparse import format as bound_before  # bound after install: the package's
    before = native_calls()
    out = sqlparse.format('select 1', keyword_case='upper'), bound_before('select 1'), sqlparse.split('a;b')
    if native_calls() - before != 3 or out != ('SELECT 1', 'select 1', ['a;', 'b']):
        print('install() did not route calls to Rust', out)
        bad += 1
    sqlparse_rs.uninstall()
    if sqlparse.format is sqlparse_rs.format or sqlparse.split is sqlparse_rs.split:
        print('uninstall() left the package in place')
        bad += 1
    print('install/uninstall: done')


sys.setprofile(hook)
corpus()
fallbacks()
lexer_changes()
installed()
sys.setprofile(None)
print('mismatches', bad)
