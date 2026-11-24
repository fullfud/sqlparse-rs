"""django-debug-toolbar's SQL panel with sqlparse_turbo.install(): its own filter stacks (the formatted SQL, and
the collapsed one with its ElideSelectListsFilter) give the HTML they give with sqlparse alone, on the corpus
(when generated), hand-picked cases, random token soups, statements too deep to group, callers near the
recursion limit; and a toolbar whose filters were changed, or a stack changed mid-run, gets sqlparse's own run.

    pip install django django-debug-toolbar && python -X utf8 tools/check_toolbar.py [cases]
"""

import glob
import html
import os
import random
import sys

import django
from django.conf import settings

settings.configure(DEBUG=True, SECRET_KEY='x', STATIC_URL='/static/',
                   INSTALLED_APPS=['django.contrib.staticfiles', 'debug_toolbar'])
django.setup()

import sqlparse  # noqa: E402
from debug_toolbar.panels.sql import utils  # noqa: E402

import sqlparse_turbo  # noqa: E402

CASES = int(sys.argv[1]) if len(sys.argv) > 1 else 5000
bad = []
counts = {}


def both(sql):
    """The formatted and the collapsed HTML, or the exception's type and text."""
    out = []
    for simplify in (False, True):
        try:
            out.append(utils.parse_sql.__wrapped__(sql, simplify=simplify))
        except Exception as e:  # noqa: BLE001
            out.append((type(e).__name__, str(e)))
    return out


def compare(label, sql):
    plain = both(sql)
    assert sqlparse_turbo.install()
    try:
        turbo = both(sql)
    finally:
        sqlparse_turbo.uninstall()
    counts[label] = counts.get(label, 0) + 1
    if plain != turbo:
        bad.append(f'{label}: {sql[:120]!r}\n  sqlparse {str(plain)[:300]}\n  turbo    {str(turbo)[:300]}')


if not sqlparse_turbo.ACTIVE:
    print(f'sqlparse {sqlparse.__version__}: the Rust does not run; nothing to check')
    sys.exit(0)

# the stacks are recognized (else this checks nothing new)
stacks = {s: utils.get_filter_stack(simplify=s) for s in (False, True)}
kinds = {s: sqlparse_turbo._toolbar(stack) for s, stack in stacks.items()}
if kinds != {False: 'formatted', True: 'simplified'}:
    bad.append(f'the toolbar stacks are not recognized: {kinds}')

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
for path in sorted(glob.glob(os.path.join(root, 'bench', 'corpus', '*.sql'))):
    compare('corpus', open(path, encoding='utf-8').read())

HAND = [
    '', ' ', '\n', ';', 'select', 'SELECT a.b', 'select a.b from', 'select a.b, c from t',
    'select a, b from t', 'SELECT COUNT(*) AS `__count` FROM t', 'select (1) as a from t',
    'select t.a; select t.b from u', 'select t.a from t; select b.c from d;\n', 'select 1;\n\n',
    'select x.y from a union select z.w from b', 'select x.y from a union all select z.w from b',
    'select x.y from a except select z.w from b intersect select q.r from c',
    'SELECT "a"."b", (SELECT c.d FROM e) FROM f WHERE g.h = \'<i>&</i>\'',
    'select a.b -- c.d from\nfrom t', 'select a.b /* from */ from t', 'select a .b from t',
    'select a. from t', 'select .a from t', 'select case when a.b then 1 end from t',
    'ſelect a.b from t', 'select a.b ſrom t', 'select a.b FROM t', 'Select A.B From T',
    "insert into t values ('<b>', '&amp;', '>')", 'select 1 < 2 and 3 > 4 & 5',
    'begin; select a.b from t; end;', 'create function f() returns int as $$ select a.b from t $$;',
    'select a.b from t go select c.d from u', 'select a.b\r\nfrom t\r\n', 'select a.b\tfrom\tt',
    'select é.ü from ½', 'select a.b from t where x in (select c.d from e)', 'select\n  a.b\nfrom t\n',
    'update t set a = 1 where b.c = 2', 'with x as (select a.b from t) select x.c from x',
]
for sql in HAND:
    compare('hand', sql)

FRAGMENTS = [
    'select', 'SELECT', 'Select', 'ſelect', 'from', 'FROM', 'ſrom', 'union', 'UNION ALL', 'except', 'intersect',
    'where', 'group by', 'order by', 'begin', 'end', 'case', 'when', 'then', 'go', 'create', 'function', 'as',
    'insert', 'into', 'values', 'join', 'on', 'and', 'or', 'not', 'null', 'is', 'in', 'between', 'over',
    '.', '.', ';', ';', '(', ')', ',', '*', '=', '<', '>', '<>', '&', '::', '[', ']', '$1', '%s', '?',
    'a', 't', 'x.y', 'a.b', '"q"', '`b`', '[c]', 'é', '½', 'count(*)',
    "'s'", "'<b>&'", "'a;b'", '$$ x.y $$', '1', '2.5', '-- c\n', '/* x */', '#h\n',
    ' ', ' ', ' ', '\n', '\t', '\r\n', '  ',
]
rng = random.Random(20261124)
for _ in range(CASES):
    parts = [rng.choice(FRAGMENTS) for _ in range(rng.randint(1, 40))]
    sep = rng.choice(['', ' ', ' ', '\n'])
    compare('random', sep.join(parts))

# too deep to group: the toolbar runs its stack again without grouping
for depth in (50, 120, 400):
    compare('deep', 'select ' + '(' * depth + 'a.b' + ')' * depth + ' from t')
compare('deep', 'select a.b from t where ' + ' and '.join(f'x{i} = {i}' for i in range(12000)))


def at_depth(n, f):
    return f() if n <= 0 else at_depth(n - 1, f)


# callers near Python's recursion limit
limit = sys.getrecursionlimit()
here = len(__import__('inspect').stack(0))
for left in (5, 15, 30, 60, 120, 300):
    sql = 'select a.b, (select c.d from e) from f where g = 1; select h.i from j'
    for simplify in (False, True):
        def call(simplify=simplify):
            try:
                return utils.parse_sql.__wrapped__(sql, simplify=simplify)
            except Exception as e:  # noqa: BLE001
                return type(e).__name__
        plain = at_depth(limit - here - left, call)
        assert sqlparse_turbo.install()
        turbo = at_depth(limit - here - left, call)
        sqlparse_turbo.uninstall()
        counts['near limit'] = counts.get('near limit', 0) + 1
        if plain != turbo:
            bad.append(f'near the limit ({left} frames left, simplify={simplify}): {plain!r} != {turbo!r}')

# a changed toolbar gets sqlparse's own run (the HTML then follows the change)
sample = 'select a.b, c.d from t where e = \'<x>\'; select f from g'
original_bold = utils.BoldKeywordFilter.process


def louder(self, stmt):
    for token in stmt.flatten():
        if token.is_keyword:
            token.value = token.value.lower()
    return original_bold(self, stmt)


utils.BoldKeywordFilter.process = louder
compare('changed: BoldKeywordFilter.process', sample)
utils.BoldKeywordFilter.process = original_bold
original_escape = utils.escape
utils.escape = lambda s, quote=True: html.escape(s, quote=True)
compare('changed: escape', sample)
utils.escape = original_escape
stacks[True].stmtprocess[0].marker = 1
compare('changed: a filter instance', sample)
del stacks[True].stmtprocess[0].marker
original_code = utils.escaped_value.__code__
utils.escaped_value.__code__ = (lambda token: token.value).__code__
compare('changed: escaped_value code', sample)
utils.escaped_value.__code__ = original_code
utils.parse_sql.cache_clear()
compare('restored', sample)
if sqlparse_turbo._toolbar(stacks[True]) != 'simplified':
    bad.append('the restored toolbar is not recognized again')

# a stack changed between statements: from that statement on, sqlparse's filters as the stack has them then


def mid_run(simplify):
    stack = sqlparse.engine.FilterStack()
    if simplify:
        stack.preprocess.append(utils.ElideSelectListsFilter())
    else:
        stack.enable_grouping()
        stack.stmtprocess.append(sqlparse.filters.AlignedIndentFilter(char='&nbsp;', n='<br/>'))
    stack.stmtprocess.append(utils.BoldKeywordFilter())
    stack.postprocess.append(utils.EscapedStringSerializer())
    out = []
    for i, stmt in enumerate(stack.run('select a.b from t; select c.d from u; select e.f from g; select 1')):
        out.append(stmt if type(stmt) is str else ('object', str(stmt)))
        if i == 0:
            stack.postprocess.clear()
        elif i == 1:
            stack._grouping = not stack._grouping
    return out


for simplify in (False, True):
    plain = mid_run(simplify)
    assert sqlparse_turbo.install()
    turbo = mid_run(simplify)
    sqlparse_turbo.uninstall()
    counts['changed mid-run'] = counts.get('changed mid-run', 0) + 1
    if plain != turbo:
        bad.append(f'stack changed mid-run (simplify={simplify}):\n  {plain}\n  {turbo}')

import debug_toolbar  # noqa: E402

print(f'django-debug-toolbar {debug_toolbar.VERSION}, Django {django.__version__}, sqlparse {sqlparse.__version__}: '
      + ', '.join(f'{k} {v}' for k, v in counts.items()))
for b in bad[:20]:
    print('FAIL', b)
print(f'{len(bad)} differ' if bad else 'all equal')
sys.exit(1 if bad else 0)
