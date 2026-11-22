"""The grouped tree and the format options that need it, original against the Rust port.

    python -X utf8 tools/check_tree.py [quick]

PYD=path loads another build of the native module (the package's copy is locked while a check runs).
Compares parse_tree() (every node: class or type, value as sqlparse keeps it, normalized, parent pointers) and
format() with grouping option sets on: the corpus, sqlparse's test files, cuts of both, random scripts made of
SQL pieces, and edge inputs (nesting limits, token limit, BETWEEN chains).
"""
import glob, importlib.util, itertools, os, random, sys, time
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, 'front'))
import sqlparse_front as py
pyd = os.environ.get('PYD') or glob.glob(os.path.join(ROOT, 'rust', 'sqlparse-front', 'python', 'sqlparse_front', '_native*'))[0]
spec = importlib.util.spec_from_file_location('_native', pyd); rs = importlib.util.module_from_spec(spec); spec.loader.exec_module(rs)
NAMES = ['keyword_case', 'identifier_case', 'strip_comments', 'use_space_around_operators', 'strip_whitespace',
         'truncate_strings', 'truncate_char', 'reindent', 'reindent_aligned', 'indent_columns', 'indent_after_first',
         'indent_tabs', 'indent_width', 'wrap_after', 'comma_first', 'compact', 'right_margin', 'output_format']
DEFAULTS = [None, None, False, False, False, None, '[...]', False, False, False, False, False, 2, 0, False, False, None, None]
FORMATS = [
    {'reindent': True}, {'reindent': True, 'keyword_case': 'upper'}, {'strip_comments': True},
    {'reindent_aligned': True}, {'use_space_around_operators': True}, {'strip_whitespace': True},
    {'reindent': True, 'comma_first': True}, {'reindent': True, 'wrap_after': 40},
    {'reindent': True, 'indent_tabs': True, 'indent_after_first': True}, {'reindent': True, 'indent_columns': True},
    {'reindent': True, 'compact': True, 'indent_width': 4}, {'reindent': True, 'comma_first': True, 'wrap_after': 1},
    {'reindent': True, 'output_format': 'python'}, {'reindent_aligned': True, 'output_format': 'php'},
    {'strip_comments': True, 'reindent': True, 'use_space_around_operators': True},
    {'reindent': True, 'reindent_aligned': True}, {'right_margin': 20},
    {'reindent_aligned': True, 'indent_tabs': True, 'strip_comments': True},
]
bad = 0
calls = 0


def run(f, *args):
    try:
        return ('ok', f(*args))
    except Exception as e:
        kind, msg = type(e).__name__, str(e)
        if kind == 'ValueError' and msg.startswith('SQLParseError: '):
            # the bridge has no SQLParseError class: it raises ValueError('SQLParseError: ...')
            kind, msg = msg.split(': ', 1)
        return ('err', kind, msg)


def report(what, text, a, b):
    global bad
    bad += 1
    if bad <= 10:
        print('MISMATCH', what, repr(text[:160]), '\n  py', repr(a)[:300], '\n  rs', repr(b)[:300], flush=True)


def check_tree(text):
    global calls
    calls += 1
    a, b = run(py.parse_tree, text), run(rs.parse_tree, text)
    if a != b:
        report('parse_tree', text, a, b)


def check_format(text, opts):
    global calls
    calls += 1
    a = run(lambda t: py.format(t, **opts), text)
    b = run(lambda t: rs.format(t, *[opts.get(n, d) for n, d in zip(NAMES, DEFAULTS)]), text)
    if a != b:
        report(opts, text, a, b)


def check(text, formats=FORMATS):
    check_tree(text)
    for o in formats:
        check_format(text, o)


def main():
    quick = 'quick' in sys.argv
    t0 = time.perf_counter()
    texts = [open(p, encoding='utf-8').read() for p in sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', '*.sql')))]
    for p in sorted(glob.glob(os.path.join(ROOT, 'upstream', 'tests', 'files', '*'))):
        try:
            texts.append(open(p, encoding='utf-8').read())
        except UnicodeDecodeError:
            pass
    if quick:
        texts = texts[::10]
    for t in texts:
        check(t)
    print(f'{len(texts)} files done, mismatches {bad}, {time.perf_counter() - t0:.0f} s', flush=True)
    r = random.Random(7)
    cuts = 0
    for t in texts:
        for _ in range(2 if quick else 6):
            a = r.randrange(len(t) + 1); b = r.randrange(a, min(len(t), a + 600) + 1)
            check(t[a:b], FORMATS if not quick else FORMATS[:6]); cuts += 1
    print(f'{cuts} cuts done, mismatches {bad}, {time.perf_counter() - t0:.0f} s', flush=True)
    pieces = ['select', 'SELECT', 'a', 'b.c', 'x.*', '"q"', '`t`', "'s'", "E'x'", '1', '2.5', '*', '+', '-', '/', '||',
              '=', '<>', '>=', 'like', 'not', 'null', 'NULL', 'is', 'and', 'or', 'between', 'in', 'as', 'AS',
              'from', 'join', 'left join', 'inner join', 'on', 'where', 'group by', 'order by', 'having', 'limit',
              'union', 'union all', 'values', 'insert into', 'update', 'set', 'delete', 'create table', 'create',
              'table', 'case', 'when', 'then', 'else', 'end', 'if', 'end if', 'for', 'end loop', 'begin', 'over',
              'partition by', 'asc', 'desc', '(', ')', '[', ']', ',', ';', '::', ':=', '->', '->>', 'date',
              "timestamp '2020-01-01'", 'interval', 'day', '@x', ':p', '?', '$1', '-- c\n', '/* m */', '/*+ hint */',
              '#x\n', '\n', '\n\n', '  ', '\t', 'f(', 'count(*)', 'at time zone', 'with', 'returning', 'into',
              'current_date', 'role', 'Σ', 'straße', 'ﬁrst', 'İ']
    n = 3000 if quick else 40000
    for i in range(n):
        k = r.randint(1, 40)
        parts = [r.choice(pieces) for _ in range(k)]
        text = ''.join(p + r.choice([' ', ' ', '', '\n']) for p in parts)
        check(text, r.sample(FORMATS, 4))
    print(f'{n} random scripts done, mismatches {bad}, {time.perf_counter() - t0:.0f} s', flush=True)
    edge = []
    for d in (99, 100, 101, 102, 150):
        edge += ['(' * d + '1' + ')' * d, 'select ' + 'case when 1 then ' * d + '2' + ' end' * d,
                 'BEGIN ' * d + 'END ' * d, 'select ' + '1 + ' * d + '1', 'select a' + ' = a' * d,
                 'select a' + ' asc' * d, 'if 1 then ' * d + 'end if; ' * d, '[' * d + ']' * d]
    edge += ['select ' + 'x, ' * 6000 + 'y', 'select ' + ' 1' * 10001, '-- a\n' * 12000 + 'select 1',
             'select a from b where c between d and e and f between g and h',
             'select 1 ' + 'between ' * 300, 'select ' + 'between ' * 50 + ' and ' * 50]
    for t in edge:
        check(t, FORMATS[:6])
    print(f'{len(edge)} edge inputs done, mismatches {bad}, calls {calls}, {time.perf_counter() - t0:.0f} s')
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
