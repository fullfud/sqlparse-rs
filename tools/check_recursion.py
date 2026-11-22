"""Where Python's recursion limit stops sqlparse, original against the Rust port.

    python -X utf8 tools/check_recursion.py            # the front door against the bindings
    python -X utf8 tools/check_recursion.py package    # sqlparse.format against sqlparse_rs.format

sqlparse bounds its own grouping depth at 100, but `@recurse` passes over ~990 nested BEGIN blocks and the
indent filters' `_next_token` over long BETWEEN chains only stop at the interpreter's recursion limit, which
FilterStack turns into SQLParseError('Maximum recursion depth exceeded'). The port reads the caller's stack
depth and models the frames sqlparse would use; this finds Python's threshold for each shape (binary search)
and checks that the port fails from exactly the same size, called from several stack depths.
"""
import glob, importlib.util, os, sys
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, 'front'))
import sqlparse_front as py
if sys.argv[1:] != ['package']:  # the front door's bindings (PYD= to pick a build, e.g. the package's own module)
    pyd = os.environ.get('PYD') or glob.glob(os.path.join(ROOT, 'rust', 'sqlparse-front', 'python', 'sqlparse_front', '_native*'))[0]
    spec = importlib.util.spec_from_file_location('_native', pyd); rs = importlib.util.module_from_spec(spec); spec.loader.exec_module(rs)
NAMES = ['keyword_case', 'identifier_case', 'strip_comments', 'use_space_around_operators', 'strip_whitespace',
         'truncate_strings', 'truncate_char', 'reindent', 'reindent_aligned', 'indent_columns', 'indent_after_first',
         'indent_tabs', 'indent_width', 'wrap_after', 'comma_first', 'compact', 'right_margin', 'output_format']
DEFAULTS = [None, None, False, False, False, None, '[...]', False, False, False, False, False, 2, 0, False, False, None, None]


def outcome(f, text):
    try:
        f(text)
        return 'ok'
    except Exception as e:
        return 'recursion' if 'recursion' in str(e) else 'other'


def at_depth(extra, f, text):
    if extra:
        return at_depth(extra - 1, f, text)
    return outcome(f, text)


def via_c(extra, f, text):
    """Like at_depth, each level a call through C (map) as well as a Python frame."""
    if extra:
        return list(map(lambda _: via_c(extra - 1, f, text), [0]))[0]
    return outcome(f, text)


PACKAGE = sys.argv[1:] == ['package']
if PACKAGE:
    import sqlparse, sqlparse_rs


def py_call(fn, opts):
    if PACKAGE:
        return lambda t: sqlparse.format(t, **opts)
    if fn == 'parse':
        return py.parse_tree
    return lambda t: py.format(t, **opts)


def rs_call(fn, opts):
    if PACKAGE:
        return lambda t: sqlparse_rs.format(t, **opts)
    if fn == 'parse':
        return rs.parse_tree
    return lambda t: rs.format(t, *[opts.get(n, d) for n, d in zip(NAMES, DEFAULTS)])


SHAPES = [
    ('nested BEGIN', 'parse', {}, lambda n: 'BEGIN ' * n + 'END ' * n),
    ('nested BEGIN x', 'parse', {}, lambda n: 'BEGIN ' * n + 'x ' + 'END ' * n),
    ('nested BEGIN, format', 'format', {'keyword_case': 'upper', 'strip_comments': True}, lambda n: 'BEGIN ' * n + 'END ' * n),
    ('nested BEGIN f()', 'parse', {}, lambda n: 'BEGIN ' * n + 'f(x) ' + 'END ' * n),
    ('BETWEEN chain, reindent', 'format', {'reindent': True}, lambda n: 'select a ' + 'between ' * n),
    ('BETWEEN chain + x, reindent', 'format', {'reindent': True}, lambda n: 'select a ' + 'between ' * n + 'x'),
    ('BETWEEN chain in parens, reindent', 'format', {'reindent': True}, lambda n: 'select (((a ' + 'between ' * n + ')))'),
    ('BETWEEN chain in where, reindent', 'format', {'reindent': True}, lambda n: 'select a from b where c ' + 'between ' * n),
    ('BETWEEN/AND chain, reindent', 'format', {'reindent': True}, lambda n: 'select a ' + 'between and ' * n),
    ('BETWEEN chain, aligned', 'format', {'reindent_aligned': True}, lambda n: 'select a ' + 'between ' * n),
    ('BETWEEN chain in parens, aligned', 'format', {'reindent_aligned': True}, lambda n: 'select (select a ' + 'between ' * n + ')'),
]


def threshold(f, shape, lo=1):
    """Smallest n where f fails with the recursion error (None if it never does up to 3000)."""
    hi = next((h for h in (1200, 3000) if at_depth(0, f, shape(h)) == 'recursion'), None)
    if hi is None:
        return None
    while lo < hi:
        mid = (lo + hi) // 2
        if at_depth(0, f, shape(mid)) == 'recursion':
            hi = mid
        else:
            lo = mid + 1
    return lo


def main():
    bad = 0
    for name, fn, opts, shape in SHAPES:
        if PACKAGE and fn == 'parse':  # the package has no parse(): sqlparse's tree is Python objects
            continue
        p, r = py_call(fn, opts), rs_call(fn, opts)
        n = threshold(p, shape)
        line = [f'{name}: python fails from {n}']
        if n is None:
            print(line[0]); continue
        for how, call in (('depth', at_depth), ('C calls', via_c)):
            for extra in (0, 7, 40):
                for k in range(n - 2 * extra - 3, n - extra + 3):
                    a, b = call(extra, p, shape(k)), call(extra, r, shape(k))
                    if a != b:
                        bad += 1
                        line.append(f'  {how}+{extra} n={k}: py {a} rs {b}')
        print('\n'.join(line), flush=True)
    print('mismatches', bad)
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
