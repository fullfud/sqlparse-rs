"""Where Python's recursion limit stops sqlparse, original against the Rust port.

    python -X utf8 tools/check_recursion.py            # the front door against the bindings
    python -X utf8 tools/check_recursion.py package    # sqlparse.format and parse against sqlparse_turbo.format and parse

sqlparse bounds its own grouping depth at 100, but `@recurse` passes over ~990 nested BEGIN blocks and the
indent filters' `_next_token` over long BETWEEN chains only stop at the interpreter's recursion limit, which
FilterStack turns into SQLParseError('Maximum recursion depth exceeded'). The port reads the caller's stack
depth and models the frames sqlparse would use; this finds Python's threshold for each shape (binary search)
and checks that the port fails from exactly the same size, called from several stack depths.

CHECK_SHARD=i/n in the environment runs only the i-th of n shares (from 0) of the shapes (tools/check_parallel.py).
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
SHARD = tuple(int(x) for x in os.environ.get('CHECK_SHARD', '0/1').split('/'))
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
    import sqlparse, sqlparse_turbo


def stack_run(run, kind):
    """A FilterStack as callers build their own (django-debug-toolbar's SQL panel: grouping, the aligned indent
    filter, a serializer), run by sqlparse's FilterStack.run or the package's replacement."""
    from sqlparse.engine import FilterStack
    from sqlparse import filters

    def call(t):
        stack = FilterStack()
        stack.enable_grouping()
        if kind == 'aligned':
            stack.stmtprocess.append(filters.AlignedIndentFilter())
        if kind == 'reindent':
            stack.stmtprocess.append(filters.ReindentFilter(wrap_after=20))
        if kind == 'list':
            return list(run(stack, t))
        stack.postprocess.append(filters.SerializerUnicode())
        return ''.join(run(stack, t))
    return call


def py_call(fn, opts):
    if PACKAGE:
        if fn.startswith('run '):
            return stack_run(sqlparse_turbo._original_run, fn[4:])
        if fn == 'split':
            return sqlparse.split
        if fn == 'parse':
            return sqlparse.parse
        return lambda t: sqlparse.format(t, **opts)
    if fn == 'parse':
        return py.parse_tree
    return lambda t: py.format(t, **opts)


def rs_call(fn, opts):
    if PACKAGE:
        if fn.startswith('run '):
            return stack_run(sqlparse_turbo._run, fn[4:])
        if fn == 'split':
            return sqlparse_turbo.split
        if fn == 'parse':
            return sqlparse_turbo.parse
        return lambda t: sqlparse_turbo.format(t, **opts)
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
RUN_SHAPES = [  # the package's FilterStack.run (install()) against sqlparse's
    ('FilterStack.run, nested BEGIN, list', 'run list', {}, lambda n: 'BEGIN ' * n + 'END ' * n),
    ('FilterStack.run, nested BEGIN f(), joined', 'run join', {}, lambda n: 'BEGIN ' * n + 'f(x) ' + 'END ' * n),
    ('FilterStack.run, nested BEGIN, aligned', 'run aligned', {}, lambda n: 'BEGIN ' * n + 'END ' * n),
    ('FilterStack.run, BETWEEN chain, aligned', 'run aligned', {}, lambda n: 'select a ' + 'between ' * n),
    ('FilterStack.run, BETWEEN chain in parens, aligned', 'run aligned', {},
     lambda n: 'select (select a ' + 'between ' * n + ')'),
    ('FilterStack.run, nested BEGIN, reindent', 'run reindent', {}, lambda n: 'BEGIN ' * n + 'END ' * n),
    ('FilterStack.run, BETWEEN chain in parens, reindent', 'run reindent', {},
     lambda n: 'select f(a, b); select (select a ' + 'between ' * n + ')'),
]


# Fixed texts whose nesting (about 90 levels, under sqlparse's grouping limit of 100) runs passes and filters
# that do not count frames; called from every depth over the last 320 frames before the limit, the outcome
# (result, or the exception's class and message) must be sqlparse's at each.
DEPTH_SHAPES = [
    ('operator chain', 'parse', {}, 'select ' + '+'.join(['a'] * 95)),
    ('operator chain, format', 'format', {'use_space_around_operators': True, 'strip_comments': True,
                                          'strip_whitespace': True}, 'select ' + '+'.join(['a'] * 95)),
    ('operator chain, reindent', 'format', {'reindent': True}, 'select ' + '+'.join(['a'] * 95)),
    ('nested parens, comments', 'parse', {}, 'select ' + '(' * 90 + 'a -- c\n' + ')' * 90),
    ('nested parens, comments, stripped', 'format', {'strip_comments': True}, 'select ' + '(' * 90 + 'a /* c */ b' + ')' * 90),
    ('nested aliases', 'parse', {}, 'select ' + '(' * 60 + 'a' + ') x' * 60),
    ('nested order by', 'parse', {}, 'select ' + '(select a from b order by c desc ' * 30 + ')' * 30),
    ('operator chain, aligned run', 'run aligned', {}, 'select ' + '+'.join(['a'] * 95)),
    ('nested calls, reindent run', 'run reindent', {},
     'select ' + 'f(' * 45 + 'a, b' + ')' * 45 + '; select g(x, y) from t where a = 1'),
    ('statements, split', 'split', {}, 'select a from t where x = 1; select 2; ' * 3),
]


def exact(f, text):
    try:
        f(text)
        return 'ok'
    except Exception as e:
        # No str(e) or formatting: up to 3.11 those C calls count against the limit this runs right at. The first
        # 32 characters: 'maximum recursion depth exceeded' (3.10 appends where Python hit it, ' in comparison',
        # a known difference, see the README's Limits) and sqlparse's 'Maximum recursion depth exceeded'.
        msg = e.args[0] if e.args and type(e.args[0]) is str else ''
        return type(e).__name__, msg[:32]


def exact_at(extra, f, text):
    if extra:
        return exact_at(extra - 1, f, text)
    return exact(f, text)


def depth_check():
    bad = 0
    here, frame = 0, sys._getframe()
    while frame:
        here, frame = here + 1, frame.f_back
    top = sys.getrecursionlimit() - here - 2  # exact_at and exact themselves still fit
    for name, fn, opts, text in DEPTH_SHAPES[SHARD[0]::SHARD[1]]:
        p, r = py_call(fn, opts), rs_call(fn, opts)
        diff = []
        for extra in range(max(0, top - 320), top):
            a, b = exact_at(extra, p, text), exact_at(extra, r, text)
            if a != b:
                diff.append((extra, a, b))
        bad += len(diff)
        print(f'{name}: {len(diff)} of the last 320 caller depths differ'
              + (f', first at {diff[0][0]}: py {diff[0][1]} | rs {diff[0][2]}' if diff else ''), flush=True)
    return bad


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
    for name, fn, opts, shape in (SHAPES + (RUN_SHAPES if PACKAGE else []))[SHARD[0]::SHARD[1]]:
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
    if PACKAGE:
        bad += depth_check()
    print('mismatches', bad)
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
