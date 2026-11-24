"""The package against sqlparse, through the public calls: sqlparse.format/split against sqlparse_turbo.format/split.

    python -X utf8 tools/check_package.py [--quick] [part ...]     parts: corpus fallbacks inert lexer installed
                                                                  cli unicode lexswap many patches stacks
                                                                  read
                                                                  (default: all)

--quick takes every 8th corpus file and every 8th chunk of code points (CI runs it on every OS x Python and
the whole check once).

CHECK_SHARD=i/n in the environment runs only the i-th of n shares (from 0) of the long parts' inputs (corpus,
unicode, lexswap, stacks, read): tools/check_parallel.py runs the shares side by side, on every core.

1. The benchmark corpus through the option sets people use, and split(): same result or same exception (class
   and message), with every call run in Rust.
2. Calls outside what was ported: each must give sqlparse's own answer and must not reach the Rust.
3. A changed default lexer goes to sqlparse; put back with default_initialization(), calls go to Rust again.
4. Every non-ASCII code point inside SQL text through the case options: the ones this Python's Unicode tables
   treat as CPython 3.11 does run in Rust (many to a call), the others (_unicode_drift.py) go to sqlparse.
5. The Rust lexer install() gives sqlparse, and sqlparse_turbo.parse: lexer.tokenize and parse() (every node:
   class, token type object, value, normalized, the is_* flags, which object its parent is) on the corpus,
   sqlparse's test files and random scripts, against sqlparse's own; parse() outside what was ported goes to
   sqlparse.
6. split_many against split, Rust and fallback texts mixed.
7. sqlparse patched (a grouping function, a filter, a method, an added method, a constant): every call goes to
   sqlparse and gives its answer, the patch included; put back, calls run in Rust again. Nesting past the
   recursion limit raises SQLParseError from a RecursionError, as sqlparse does.
8. FilterStack.run (install() replaces it): stacks callers build themselves (grouping, the indent filters, a
   filter of their own, serializers) against sqlparse's run, trees and output; stacks with token-stream filters
   or without grouping stay sqlparse's. A caller's AlignedIndentFilter run in Rust: trees (the TokenList it
   wraps the statement in included), output and the filter's state after the run.
9. Text streams and bytes (read and decoded as sqlparse's lexer does) through format, split, parse,
   parsestream, the lexer, split_many and a caller's stack: same result or error as sqlparse, the stream read
   as many times, and in Rust.

Exits 1 on any mismatch, so CI fails on it.
"""
import glob
import os
import sys
import time
import unicodedata

import sqlparse
import sqlparse_turbo
from sqlparse import keywords, tokens
from sqlparse.lexer import Lexer

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NATIVE = {sqlparse_turbo._native.format: 'format', sqlparse_turbo._native.split: 'split',
          sqlparse_turbo._native.lex: 'lex', sqlparse_turbo._native.split_many: 'split_many',
          sqlparse_turbo._native.parse: 'parse', sqlparse_turbo._native.statements: 'parse'}
counts = {'format': 0, 'split': 0, 'lex': 0, 'split_many': 0, 'parse': 0}
SHARD = tuple(int(x) for x in os.environ.get('CHECK_SHARD', '0/1').split('/'))


def mine(items):
    """This share of `items` (all of them without CHECK_SHARD)."""
    return list(items)[SHARD[0]::SHARD[1]]


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


def plain(f, *args, **kwargs):
    """outcome() with the profile hook off: it counts C calls only, and slows sqlparse's Python several times."""
    sys.setprofile(None)
    try:
        return outcome(f, *args, **kwargs)
    finally:
        sys.setprofile(hook)


bad = 0


def same(what, a, b):
    global bad
    if a != b:
        bad += 1
        if bad <= 10:
            print('MISMATCH', what, '\n  sqlparse   ', repr(a)[:300], '\n  sqlparse_turbo', repr(b)[:300])


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
    paths = sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', 'q*.sql')))
    if not paths:
        sys.exit('no corpus: run python bench/gen_corpus.py first')
    texts = [open(p, encoding='utf-8').read() for p in paths[::8 if QUICK else 1]]
    texts += ['', ';', 'select 1; select 2;', "select 'unclosed", 'BEGIN ' * 990 + 'END ' * 990,
              'select a ' + 'between ' * 980]
    texts = mine(texts)
    before = native_calls()
    n = uncounted = 0
    t_py = t_rs = 0.0
    for opts in OPTION_SETS:
        for t in texts:
            t0 = time.perf_counter()
            a = plain(sqlparse.format, t, **opts)
            t1 = time.perf_counter()
            rearm()
            b = outcome(sqlparse_turbo.format, t, **opts)
            t2 = time.perf_counter()
            uncounted += rearm()
            t_py += t1 - t0
            t_rs += t2 - t1
            same(f'format {opts} {t[:60]!r}', a, b)
            n += 1
    for t in texts:
        for strip in (False, True):
            a = plain(sqlparse.split, t, strip_semicolon=strip)
            rearm()
            same(f'split {strip} {t[:60]!r}', a, outcome(sqlparse_turbo.split, t, strip_semicolon=strip))
            uncounted += rearm()
            n += 1
    ran = native_calls() - before
    print(f'corpus: {n} calls compared, {ran} counted in Rust, {uncounted} not countable (the hook hit the '
          f'recursion limit); format: sqlparse {t_py:.1f} s, sqlparse_turbo {t_rs:.1f} s')
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
        ('bytearray', (bytearray(sql.encode()),), {}),
        ('bytes, an option of another type', (sql.encode(),), {'reindent': 1}),
        ('str subclass', (Sub(sql),), {'reindent': True}),
        # The two below do reach the Rust: the text cannot become UTF-8, the call raises UnicodeEncodeError before
        # any work, and the package hands it to sqlparse.
        ('lone surrogate', ('select \ud800 from t',), {'keyword_case': 'upper'}),
        ('lone surrogate in option', (sql,), {'truncate_strings': 3, 'truncate_char': '\udc80'}),
        ('stream, an option of another type', (__import__('io').StringIO(sql),), {'reindent': 1}),
    ]
    for name, args, opts in cases:
        rearm()
        before = native_calls()
        b = outcome(sqlparse_turbo.format, *args, **opts)
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
                               ('bytes, strip_semicolon=1', (sql.encode(),), {'strip_semicolon': 1})]:
        before = native_calls()
        b = outcome(sqlparse_turbo.split, *args, **kwargs)
        ran = native_calls() - before
        same(f'split fallback {name}', outcome(sqlparse.split, *args, **kwargs), b)
        if ran:
            print('REACHED RUST', name)
            bad += 1
    print(f'fallbacks: {len(cases) + 3} calls, all answered by sqlparse' if not bad else 'fallbacks: see above')


def inert_arguments():
    """What sqlparse never reads for a str: `encoding` (it only decodes bytes) and keywords that are no format
    option (`indent_char` it overwrites). The package runs these in Rust, and they must give sqlparse's answer."""
    global bad
    sql = "select a, b from t where x = 1 -- note\n; select 'two'"
    cases = [
        ('format, encoding', sqlparse.format, sqlparse_turbo.format, (sql, 'utf-16'), {'reindent': True}),
        ('format, encoding object', sqlparse.format, sqlparse_turbo.format, (sql, object()), {}),
        ('format, unknown keyword', sqlparse.format, sqlparse_turbo.format, (sql,), {'no_such_option': [1]}),
        ('format, indent_char', sqlparse.format, sqlparse_turbo.format, (sql,), {'reindent': True, 'indent_char': 'x'}),
        ('format, CLI-style keywords', sqlparse.format, sqlparse_turbo.format, (sql,),
         {'filename': ['-'], 'outfile': None, 'inplace': False, 'encoding': 'utf-8', 'reindent': True,
          'indent_char': ' ', 'keyword_case': 'upper'}),
        ('split, encoding', sqlparse.split, sqlparse_turbo.split, (sql, 'latin-1'), {}),
    ]
    for name, f_py, f_rs, args, kwargs in cases:
        rearm()
        before = native_calls()
        b = outcome(f_rs, *args, **kwargs)
        ran = native_calls() - before
        same(name, outcome(f_py, *args, **kwargs), b)
        if ran != 1:
            print('NOT IN RUST', name)
            bad += 1
    print(f'inert arguments: {len(cases)} calls, all in Rust')


def cli():
    """`sqlformat-turbo` against sqlparse's own `sqlformat`: same output, format() run in Rust."""
    global bad
    import contextlib, io, tempfile
    from sqlparse import cli as sqlcli
    import sqlparse_turbo.__main__ as rscli
    path = sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', 'q*.sql')))[0]
    for args in (['-r', '-k', 'upper', path], ['--reindent_aligned', '-s', path], ['-r', '--comma_first', 'True', path],
                 ['-k', 'nope', path]):
        outs = []
        for main in (sqlcli.main, rscli.main):
            rearm()
            before = native_calls()
            buf, err = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(err):
                try:
                    code = main(list(args))
                except SystemExit as e:  # argparse rejects a bad choice this way
                    code = ('exit', e.code)
            outs.append((code, buf.getvalue(), err.getvalue(), native_calls() - before))
            sqlparse_turbo.uninstall()
        (c1, o1, e1, _), (c2, o2, e2, ran) = outs
        same(f'cli {args[:-1]}', (c1, o1, e1), (c2, o2, e2))
        if c1 == 0 and ran != 1:
            print('CLI NOT IN RUST', args)
            bad += 1
    print('cli: done')


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
        for f_py, f_rs, kw in ((sqlparse.format, sqlparse_turbo.format, {'reindent': True, 'keyword_case': 'upper'}),
                               (sqlparse.split, sqlparse_turbo.split, {})):
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
             outcome(sqlparse_turbo.format, sql, reindent=True))
        if native_calls() - before != 1:
            print('NOT IN RUST after default_initialization', name)
            bad += 1
    print('lexer changes: done')


def unicode_sweep():
    """Each non-ASCII scalar value inside a few SQL contexts, under the options whose result depends on Unicode
    tables (case, \\w for names and keywords, splitlines, sigma). Code points that should run in Rust go 40 to a
    text; drifted ones one at a time, each of which must stay with sqlparse."""
    global bad
    from sqlparse_turbo import _unicode_drift
    version = unicodedata.unidata_version
    if version == _unicode_drift.PORTED_UNICODE:
        drift = ()
    else:
        drift = _unicode_drift.DRIFT.get(version)
    if drift is None:  # a Unicode version the package does not know: only ASCII text may run in Rust
        drift = ((0x80, 0x10FFFF),)
    drifted = set()
    for a, b in drift:
        drifted.update(range(a, b + 1))
    ported = [chr(cp) for cp in range(0x80, 0x110000) if not 0xD800 <= cp <= 0xDFFF and cp not in drifted]
    contexts = ['select {} from t', 'select a{}b, "{}"Σ, x{}Σ from {} where {} = 1']
    options = [{'keyword_case': 'upper', 'identifier_case': 'lower'}, {'keyword_case': 'capitalize'},
               {'identifier_case': 'capitalize', 'reindent': True}]
    n = ran = lexed = 0
    t0 = time.perf_counter()
    for i in mine(range(0, len(ported), 320 if QUICK else 40)):
        chunk = ''.join(ported[i:i + 40])
        for ctx in contexts:
            text = ctx.replace('{}', chunk)
            for opts in options:
                rearm()
                before = native_calls()
                b = outcome(sqlparse_turbo.format, text, **opts)
                ran += native_calls() - before
                same(f'unicode {opts} U+{ord(chunk[0]):04X}..', plain(sqlparse.format, text, **opts), b)
                n += 1
            lexed += lexer_same(f'unicode lexer U+{ord(chunk[0]):04X}..', text)
    if ran != n or lexed != n // len(options):
        print(f'UNICODE NOT ALL IN RUST: format {ran} of {n}, lexer {lexed} of {n // len(options)}')
        bad += 1
    leaked = 0
    for cp in mine(sorted(drifted)[::max(1, len(drifted) // 3000)]):
        text = f'select a{chr(cp)}b from t'
        before = native_calls()
        b = outcome(sqlparse_turbo.format, text, keyword_case='upper')
        leaked += native_calls() - before
        same(f'unicode drift U+{cp:04X}', outcome(sqlparse.format, text, keyword_case='upper'), b)
        b = outcome(sqlparse_turbo.split, text + ';x')
        leaked += native_calls() - before
        same(f'unicode drift split U+{cp:04X}', outcome(sqlparse.split, text + ';x'), b)
    if leaked:
        print(f'UNICODE DRIFT REACHED RUST {leaked} times')
        bad += 1
    share = f' (share {SHARD[0] + 1} of {SHARD[1]})' if SHARD[1] > 1 else ''
    print(f'unicode {version}{share}: {len(ported)} code points in {n} calls in Rust, {len(drifted)} drifted ones '
          f'left to sqlparse, {time.perf_counter() - t0:.0f} s')


def installed():
    global bad
    rearm()
    assert sqlparse_turbo.install()
    from sqlparse import format as bound_before  # bound after install: the package's
    before = native_calls()
    out = sqlparse.format('select 1', keyword_case='upper'), bound_before('select 1'), sqlparse.split('a;b')
    if native_calls() - before != 3 or out != ('SELECT 1', 'select 1', ['a;', 'b']):
        print('install() did not route calls to Rust', out)
        bad += 1
    sqlparse_turbo.uninstall()
    if (sqlparse.format is sqlparse_turbo.format or sqlparse.split is sqlparse_turbo.split
            or Lexer.get_tokens is sqlparse_turbo._get_tokens):
        print('uninstall() left the package in place')
        bad += 1
    print('install/uninstall: done')


def tree(node, holder=None):
    """parse()'s tree as plain data: every slot of every node (the token type as the object itself), and its
    parent: the group holding it, None, or another node (named by class and value)."""
    if node.parent is holder:
        parent = 'holder'
    elif node.parent is None:
        parent = None
    else:
        parent = ('other', type(node.parent).__name__, node.parent.value)
    slots = (type(node).__name__, node.ttype, node.value, node.normalized, node.is_group, node.is_keyword,
             node.is_whitespace, node.is_newline, parent)
    if node.is_group:
        return slots + ([tree(t, node) for t in node.tokens],)
    return slots


def lexer_same(what, text):
    """tokenize and parse() of `text` with the Rust lexer installed against sqlparse's; 1 when the Rust lexed
    it (once per call), else 0."""
    global bad
    from sqlparse import lexer
    a = plain(lambda: [(tt, v) for tt, v in lexer.tokenize(text)])
    ta = plain(lambda: [tree(s) for s in sqlparse.parse(text)])
    sqlparse_turbo.install()
    rearm()
    before = counts['lex'], counts['parse']
    b = outcome(lambda: [(tt, v) for tt, v in lexer.tokenize(text)])
    tb = outcome(lambda: [tree(s) for s in sqlparse.parse(text)])
    ran = (counts['lex'] - before[0], counts['parse'] - before[1])
    sqlparse_turbo.uninstall()
    if tb[0] == 'ok' and ta[0] == 'ok' and type(sqlparse_turbo.parse(text)) is not type(sqlparse.parse(text)):
        print('PARSE RETURNS ANOTHER TYPE', what)
        bad += 1
    same(what, a, b)
    same(what + ' parse', ta, tb)
    if a == b and a[0] == 'ok' and any(x[0] is not y[0] for x, y in zip(a[1], b[1])):
        print('OTHER TOKEN TYPE OBJECTS', what)
        bad += 1
    return 1 if ran == (1, 1) else 0


def lexer_swap():
    import random
    paths = sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', '*.sql')))[::8 if QUICK else 1]
    texts = [open(p, encoding='utf-8').read() for p in paths]
    for p in sorted(glob.glob(os.path.join(ROOT, 'upstream', 'tests', 'files', '*'))):
        try:
            texts.append(open(p, encoding='utf-8').read())
        except UnicodeDecodeError:
            pass
    src = open(os.path.join(ROOT, 'tools', 'check_tree.py'), encoding='utf-8').read()
    start = src.index('pieces = [')
    pieces = __import__('ast').literal_eval(src[start + len('pieces = '):src.index(']\n', start) + 1])
    r = random.Random(11)
    for _ in range(300 if QUICK else 3000):
        texts.append(''.join(r.choice(pieces) + r.choice([' ', '', '\n']) for _ in range(r.randint(1, 40))))
    texts += ['', ' ', 'select $$a$$ /* c */ --x\n$t$b$t$', "select 'unclosed", 'BEGIN ' * 120 + 'END ' * 120]
    texts = mine(texts)
    if not sqlparse_turbo._native.objects_direct():  # the trees below are made the direct way
        print('sqlparse objects not made directly (slots layout not found)')
        bad_add()
    lexed = sum(lexer_same(f'lexer swap {t[:50]!r}', t) for t in texts)
    # what the swap must leave to sqlparse's lexer
    from sqlparse import lexer
    sql = 'select a, b from t'
    sqlparse_turbo.install()
    before = counts['lex']
    other = Lexer()
    other.default_initialization()
    list(other.get_tokens(sql))
    lx = Lexer.get_default_instance()
    lx.add_keywords({'FOO': tokens.Keyword})
    list(lexer.tokenize(sql))
    changed = [tree(s) for s in sqlparse.parse(sql)]
    lx.default_initialization()
    leaked = counts['lex'] - before
    for args in ((Sub(sql),), (bytearray(sql.encode()),), ('select \ud800',)):
        before = counts['parse']
        a = plain(lambda: [tree(s) for s in sqlparse_turbo._original_parse(*args)])
        same(f'parse fallback {args!r}', a, outcome(lambda: [tree(s) for s in sqlparse.parse(*args)]))
        # a lone surrogate may reach the Rust, which cannot take it as UTF-8 and hands it to sqlparse
        if args[0] != 'select \ud800':
            leaked += counts['parse'] - before
    lx.add_keywords({'FOO': tokens.Keyword})
    before = counts['parse']
    same('parse with a changed lexer', ('ok', changed), outcome(lambda: [tree(s) for s in sqlparse.parse(sql)]))
    leaked += counts['parse'] - before
    lx.default_initialization()
    sqlparse_turbo.uninstall()
    if leaked or lexed != len(texts):
        print(f'LEXER SWAP: {lexed} of {len(texts)} texts lexed in Rust, {leaked} calls that should not have been')
        bad_add()
    print(f'lexer swap: {len(texts)} texts, tokens and parse trees compared, all lexed in Rust')


def bad_add():
    global bad
    bad += 1


def many():
    paths = sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', 'q*.sql')))[::8 if QUICK else 1]
    texts = [open(p, encoding='utf-8').read() for p in paths]
    mixed = texts + [b'select 1; select 2', Sub('a;b'), 'x\ud800; y', '', ';;', 'select \U00031350; x']
    for strip in (False, True):
        expect = [plain(sqlparse.split, t, strip_semicolon=strip) for t in mixed]
        before = counts['split_many']
        got = outcome(sqlparse_turbo.split_many, mixed, strip_semicolon=strip)
        same(f'split_many strip={strip}', ('ok', [e[1] for e in expect]), got)
        if counts['split_many'] - before != 1:
            print('split_many did not run in Rust')
            bad_add()
    print(f'split_many: {len(mixed)} texts x 2, same as split')


def patches():
    import sqlparse.engine.grouping as grouping
    import sqlparse.filters.reindent as reindent
    sql = 'select a, b as c from t where x = 1 and y in (select 2) order by a; select f(x) from u'
    calls = [
        ('format', lambda m: m.format(sql, reindent=True, keyword_case='upper')),
        ('split', lambda m: m.split(sql)),
        ('parse', lambda m: [tree(s) for s in m.parse(sql)]),
    ]
    orig_identifier = grouping.group_identifier
    orig_next = sqlparse.sql.TokenList.token_next
    orig_nl = reindent.ReindentFilter.nl

    def wrapped_identifier(tlist):
        return orig_identifier(tlist)

    def no_identifiers(tlist):
        return None

    changes = [
        ('grouping function replaced', lambda: setattr(grouping, 'group_identifier', no_identifiers),
         lambda: setattr(grouping, 'group_identifier', orig_identifier)),
        ('grouping function wrapped', lambda: setattr(grouping, 'group_identifier', wrapped_identifier),
         lambda: setattr(grouping, 'group_identifier', orig_identifier)),
        ('method replaced', lambda: setattr(sqlparse.sql.TokenList, 'token_next',
                                            lambda self, idx, skip_ws=True, skip_cm=False, _reverse=False:
                                            orig_next(self, idx, False, skip_cm, _reverse)),
         lambda: setattr(sqlparse.sql.TokenList, 'token_next', orig_next)),
        ('method added', lambda: setattr(sqlparse.sql.Identifier, 'get_alias', lambda self: None),
         lambda: delattr(sqlparse.sql.Identifier, 'get_alias')),
        ('filter method replaced', lambda: setattr(reindent.ReindentFilter, 'nl',
                                                   lambda self, offset=0: sqlparse.sql.Token(tokens.Newline, '\n')),
         lambda: setattr(reindent.ReindentFilter, 'nl', orig_nl)),
        ('constant changed', lambda: setattr(grouping, 'MAX_GROUPING_TOKENS', 5),
         lambda: setattr(grouping, 'MAX_GROUPING_TOKENS', int('10000'))),
    ]
    for name, change, undo in changes:
        change()
        try:
            for what, call in calls:
                expect = plain(lambda: call(sqlparse))
                before = sum(counts.values())
                got = outcome(lambda: call(sqlparse_turbo))
                ran = sum(counts.values()) - before
                same(f'patched ({name}) {what}', expect, got)
                if ran:
                    print(f'PATCHED ({name}) {what} REACHED RUST')
                    bad_add()
        finally:
            undo()
        for what, call in calls:
            before = sum(counts.values())
            same(f'put back ({name}) {what}', plain(lambda: call(sqlparse)), outcome(lambda: call(sqlparse_turbo)))
            if sum(counts.values()) - before == 0:
                print(f'PUT BACK ({name}) {what} NOT IN RUST')
                bad_add()
    deep = 'BEGIN ' * 1500 + 'END ' * 1500
    for what, a, b in (('parse', sqlparse.parse, sqlparse_turbo.parse),
                       ('format', lambda t: sqlparse.format(t, reindent=True),
                        lambda t: sqlparse_turbo.format(t, reindent=True))):
        errs = []
        for f in (a, b):
            try:
                f(deep)
                errs.append(None)
            except Exception as e:
                cause = e.__cause__ and e.__cause__.args
                if cause and sys.version_info < (3, 11):
                    # 3.10 names the C call that hit the limit ('... in comparison'), which the port cannot know
                    cause = tuple(a.split(' in ')[0] if isinstance(a, str) else a for a in cause)
                errs.append((type(e), e.args, type(e.__cause__), cause,
                             e.__suppress_context__, e.__context__ is e.__cause__))
        same(f'recursion error chain ({what})', ('ok', errs[0]), ('ok', errs[1]))
    print(f'patches: {len(changes)} kinds of patch x {len(calls)} calls, answered by sqlparse; put back, Rust')


def stacks():
    """FilterStack.run with install() (django-debug-toolbar builds its own stack): stacks with grouping on and no
    token-stream filters get Rust-grouped statements and run their filters in Python; the others are sqlparse's
    own run. Trees and output against sqlparse's run, and which of them reached the Rust."""
    from sqlparse import filters
    from sqlparse.engine import FilterStack
    paths = sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', '*.sql')))[::8 if QUICK else 1]
    texts = [open(p, encoding='utf-8').read() for p in paths]
    texts += ['', ' ', 'select 1;select 2; ;', "select 'unclosed", '/* c */ select a from b -- x',
              'BEGIN ' * 120 + 'END ' * 120, 'select a ' + 'between ' * 60, 'sélect ñ from 表']
    texts = mine(texts)

    class Bold:  # a caller's own statement filter, as the toolbar's BoldKeywordFilter
        def process(self, stmt):
            idx = 0
            while idx < len(stmt.tokens):
                token = stmt.tokens[idx]
                if token.is_keyword:
                    stmt.insert_before(idx, sqlparse.sql.Token(tokens.Other, '<b>'))
                    stmt.insert_after(idx + 1, sqlparse.sql.Token(tokens.Other, '</b>'))
                    idx += 2
                elif token.is_group:
                    self.process(token)
                idx += 1

    def grouping_off(stack):
        """A filter that turns the stack's grouping off (sqlparse's run looks at it for every statement)."""
        class Off:
            __slots__ = ()

            def process(self, stmt):
                stack._grouping = False
        return Off()

    def build(kind):
        stack = FilterStack()
        if kind != 'no grouping':
            stack.enable_grouping()
        if kind == 'aligned':
            stack.stmtprocess.append(filters.AlignedIndentFilter(char='&nbsp;', n='<br/>'))
            stack.stmtprocess.append(Bold())
        elif kind in ('aligned trees', 'aligned first'):
            stack.stmtprocess.append(filters.AlignedIndentFilter())
        elif kind == 'aligned state':
            f = filters.AlignedIndentFilter(char='\t', n='\r\n')
            f.offset, f.indent, f._max_kwd_len = 3, 1, 9
            stack.stmtprocess.append(f)
        elif kind in ('reindent', 'reindent trees', 'reindent first'):
            stack.stmtprocess.append(filters.ReindentFilter())
        elif kind == 'reindent html':  # as a page would show it, with a filter of the caller's own after it
            stack.stmtprocess.append(filters.ReindentFilter(width=4, n='<br/>\n', wrap_after=30))
            stack.stmtprocess.append(Bold())
        elif kind == 'reindent nbsp':
            # A `char` longer than one character: sqlparse measures a line in characters but an indent in units,
            # so its offsets grow with every nesting level until the indent no longer fits in memory (the Python
            # filter's MemoryError); short texts only.
            stack.stmtprocess.append(filters.ReindentFilter(char='&nbsp;', n='<br/>', wrap_after=40))
        elif kind == 'reindent options':
            stack.stmtprocess.append(filters.ReindentFilter(
                width=3, wrap_after=12, comma_first=True, indent_after_first=True, indent_columns=True,
                compact=True))
        elif kind == 'reindent tabs trees':
            f = filters.ReindentFilter(char='\t', wrap_after=25, n='\r\n')
            f.offset = 2
            stack.stmtprocess.append(f)
        elif kind == 'grouping off':
            stack.stmtprocess.append(grouping_off(stack))
        elif kind == 'strip comments':
            stack.stmtprocess.append(filters.StripCommentsFilter())
        elif kind == 'preprocess':
            stack.preprocess.append(filters.KeywordCaseFilter('upper'))
        if kind not in ('trees', 'aligned trees', 'aligned first', 'reindent trees', 'reindent first',
                        'reindent tabs trees', 'grouping off'):
            stack.postprocess.append(filters.SerializerUnicode())
        return stack

    def wrapped(s):
        """The TokenList AlignedIndentFilter wraps a statement's tokens in: its slots, and whether it holds the
        statement's own list."""
        holders = {id(t.parent): t.parent for t in s.tokens if t.parent is not s}
        return [(type(h).__name__, h.ttype, h.value, h.normalized, h.is_group, h.is_keyword, h.is_whitespace,
                 h.is_newline, h.parent, h.tokens is s.tokens, len(h.tokens)) for h in holders.values()]

    def result(run, kind, text):
        stack = build(kind)
        filters_ = list(stack.stmtprocess)
        if kind in ('trees', 'aligned trees', 'reindent trees', 'reindent tabs trees', 'grouping off'):
            out = [(tree(s), wrapped(s)) for s in run(stack, text)]
        elif kind in ('first', 'aligned first', 'reindent first'):
            got = []
            try:  # a statement, then the error (if any) of the next: sqlparse groups one at a time
                for s in run(stack, text):
                    got.append(tree(s))
                    if len(got) == 2:
                        break
            except Exception as e:
                got.append((type(e).__name__, str(e)))
            out = got
        else:
            out = ''.join(run(stack, text))
        return out, [{k: tree(v) if isinstance(v, sqlparse.sql.Token) else v for k, v in vars(f).items()}
                     if hasattr(f, '__dict__') else None for f in filters_], last_func_in_tree(filters_, stack)

    def last_func_in_tree(filters_, stack):
        """Whether a ReindentFilter's _last_func is the very token of its last statement (not an equal copy)."""
        f = filters_[0] if filters_ else None
        if type(f) is not filters.ReindentFilter or f._last_func is None or f._last_stmt is None:
            return None
        found, todo = False, [f._last_stmt]
        while todo and not found:
            t = todo.pop()
            found = t is f._last_func
            if t.is_group:
                todo.extend(t.tokens)
        return found

    kinds = ['trees', 'aligned', 'aligned trees', 'aligned state', 'aligned first', 'reindent', 'reindent trees',
             'reindent first', 'reindent html', 'reindent nbsp', 'reindent options', 'reindent tabs trees',
             'grouping off', 'strip comments', 'first', 'preprocess', 'no grouping']
    grouped = 0
    for text in texts:
        for kind in kinds:
            if kind == 'reindent nbsp' and len(text) > 200:
                continue
            expect = plain(lambda: result(sqlparse_turbo._original_run, kind, text))
            before = counts['parse']
            got = outcome(lambda: result(sqlparse_turbo._run, kind, text))
            ran = counts['parse'] - before
            same(f'stack {kind} {text[:40]!r}', expect, got)
            should = kind not in ('preprocess', 'no grouping') and sqlparse_turbo._text_ported(text)
            if ran != (1 if should else 0):
                print(f'STACK {kind} {text[:40]!r}: {ran} Rust parses, expected {int(should)}')
                bad_add()
            grouped += ran
    gen = sqlparse_turbo._run(build('trees'), 'select 1')
    if type(gen).__name__ != 'generator':
        print('FilterStack.run replacement is not a generator')
        bad_add()
    sqlparse_turbo.install()
    try:
        if FilterStack.run is not sqlparse_turbo._run or not sqlparse_turbo._sqlparse_untouched():
            print('install() did not put run in place, or the watch saw it as a change')
            bad_add()
    finally:
        sqlparse_turbo.uninstall()
    # the Rust runs the filter itself on ordinary statements
    state = ('\n', ' ', 0, 0, 6)
    st = sqlparse_turbo._native.statements('select a, b from t where x = 1; select (select 1) from u')
    if [st.next(state)[1], st.next(state)[1], st.next(state)] != [True, True, None]:
        print('AlignedIndentFilter not run in Rust')
        bad_add()
    state = ('\n', ' ', 2, 0, 0, 0, False, False, False, None, None)
    st = sqlparse_turbo._native.statements('select f(a, b) from t where x = 1; select (select 1) from u')
    first, second = st.next(None, state), st.next(None, state)
    if [first[1], second[1], st.next(None, state)] != [True, True, None] or first[2] is None or second[2] is not None:
        print('ReindentFilter not run in Rust, or its _last_func not given back')
        bad_add()
    print(f'stacks: {len(texts)} texts x {len(kinds)} stacks (nbsp: the short ones), same as sqlparse run; '
          f'{grouped} grouped in Rust')


def read_inputs():
    """Text streams and bytes: sqlparse's lexer reads the stream and decodes the bytes (encoding, else UTF-8, else
    unicode-escape) before lexing; the package does the same, then runs the text in Rust."""
    import io
    from sqlparse import lexer
    from sqlparse.engine import FilterStack
    from sqlparse import filters

    class Counted(io.TextIOBase):  # a stream that counts its reads and gives what it is told to
        def __init__(self, value, fail=False):
            self.value, self.fail, self.reads = value, fail, 0

        def read(self, size=-1):
            self.reads += 1
            if self.fail:
                raise ValueError('cannot read')
            return self.value

    paths = sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', '*.sql')))[::16 if QUICK else 2]
    texts = [open(p, encoding='utf-8').read() for p in paths]
    texts += ['', 'select 1; select 2', 'sélect ñ from 表 -- é', 'BEGIN ' * 120 + 'END ' * 120]
    texts = mine(texts)
    forms = [
        ('bytes', lambda t: (t.encode(), None), True),
        ('bytes latin-1', lambda t: (t.encode('latin-1', 'replace'), 'latin-1'), True),
        ('bytes utf-16', lambda t: (t.encode('utf-16'), 'utf-16'), True),
        ('bytes not utf-8', lambda t: (t.encode() + b' \xff', None), True),
        ('bytes bad escape', lambda t: (t.encode() + b' \\x \xff', None), False),
        ('bytes bad encoding name', lambda t: (t.encode(), 'no-such-codec'), False),
        ('StringIO', lambda t: (io.StringIO(t), None), True),
        ('TextIOWrapper', lambda t: (io.TextIOWrapper(io.BytesIO(t.encode('utf-8'))), 'ignored'), True),
        ('stream of bytes', lambda t: (Counted(t.encode()), None), True),
        ('stream of str', lambda t: (Counted(t), None), True),
        ('stream of an int', lambda t: (Counted(5), None), False),
        ('stream that fails', lambda t: (Counted(t, fail=True), None), False),
        ('bytearray', lambda t: (bytearray(t.encode()), None), False),
    ]

    def aligned_stack():
        stack = FilterStack()
        stack.enable_grouping()
        stack.stmtprocess.append(filters.AlignedIndentFilter(char='&nbsp;', n='<br/>'))
        stack.postprocess.append(filters.SerializerUnicode())
        return stack

    calls = [
        ('format', lambda m, x, e: m.format(x, e, reindent=True, keyword_case='upper'), 'format'),
        ('format bad option', lambda m, x, e: m.format(x, e, keyword_case='nope'), None),
        ('split', lambda m, x, e: m.split(x, e), 'split'),
        ('split strip', lambda m, x, e: m.split(x, e, strip_semicolon=True), 'split'),
        ('parse', lambda m, x, e: [tree(s) for s in m.parse(x, e)], 'parse'),
        ('parsestream', lambda m, x, e: [tree(s) for s in sqlparse.parsestream(x, e)], 'parse'),
        ('tokenize', lambda m, x, e: list(lexer.tokenize(x, e)), 'lex'),
        ('split_many', lambda m, x, e: m.split_many([x, 'a;b'], e) if m is sqlparse_turbo
         else [sqlparse.split(x, e), sqlparse.split('a;b', e)], 'split'),
        ('stack', lambda m, x, e: ''.join(aligned_stack().run(x, e)), 'parse'),
    ]
    n = rust = 0
    for text in texts:
        for form, make, runs in forms:
            for what, call, counter in calls:
                x, enc = make(text)
                expect = plain(lambda: call(sqlparse, x, enc))
                reads_a = getattr(x, 'reads', None)
                where_a = x.tell() if isinstance(x, io.StringIO) else None
                x, enc = make(text)
                sqlparse_turbo.install()
                rearm()
                before = dict(counts)
                try:
                    got = outcome(lambda: call(sqlparse_turbo, x, enc))
                finally:
                    sqlparse_turbo.uninstall()
                reads_b = getattr(x, 'reads', None)
                where_b = x.tell() if isinstance(x, io.StringIO) else None
                same(f'read {form} {what} {text[:30]!r}', (expect, reads_a, where_a), (got, reads_b, where_b))
                n += 1
                if counter and runs and expect[0] == 'ok' and 'BEGIN' not in text:
                    if counts[counter] == before[counter] and (counter != 'split' or
                                                               counts['split_many'] == before['split_many']):
                        print(f'READ {form} {what} {text[:30]!r} DID NOT RUN IN RUST')
                        bad_add()
                    rust += 1
    print(f'read: {n} calls on streams and bytes, same as sqlparse (result, error, reads); {rust} in Rust')


PARTS = {'corpus': corpus, 'fallbacks': fallbacks, 'inert': inert_arguments, 'lexer': lexer_changes,
         'installed': installed, 'cli': cli, 'unicode': unicode_sweep, 'lexswap': lexer_swap, 'many': many,
         'patches': patches, 'stacks': stacks, 'read': read_inputs}
args = sys.argv[1:]
QUICK = '--quick' in args
chosen = [a for a in args if a != '--quick'] or list(PARTS)
sys.setprofile(hook)
for part in chosen:
    t0 = time.perf_counter()
    PARTS[part]()
    print(f'  [{part} {time.perf_counter() - t0:.0f} s]', flush=True)
sys.setprofile(None)
print('mismatches', bad)
sys.exit(1 if bad else 0)
