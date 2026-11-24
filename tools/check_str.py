"""str() of sqlparse's statements with sqlparse_turbo.install(): on CPython 3.12+ TokenList.__str__ reads the
leaves in Rust; it must give what sqlparse's own gives (the same string, the very same object for one leaf, the
same exception) on every statement of the corpus and its sub-groups, and on trees a caller changed: values
replaced, tokens inserted or removed, a token of the caller's own class, is_group or tokens of other types, an
unset slot, a cycle, a deep tree, a patched flatten. Also the stack serializers the Rust runs.

    python -X utf8 tools/check_str.py
"""

import glob
import io
import os
import sys

import sqlparse
from sqlparse import filters, sql, tokens as T
from sqlparse.engine import FilterStack

import sqlparse_turbo

if not sqlparse_turbo.ACTIVE:
    print(f'sqlparse {sqlparse.__version__}: the Rust does not run; nothing to check')
    sys.exit(0)

original = sqlparse_turbo._original_str
bad = []
counts = {}


def outcome(f, obj):
    try:
        r = f(obj)
        return ('ok', r, None)
    except Exception as e:  # noqa: BLE001
        return ('raise', type(e).__name__, str(e))


def compare(label, obj):
    counts[label] = counts.get(label, 0) + 1
    want = outcome(original, obj)
    got = outcome(str, obj)
    if want != got:
        bad.append(f'{label}: {str(want)[:200]} != {str(got)[:200]}')
    elif want[0] == 'ok' and len(list(obj.flatten())) == 1 and (original(obj) is not str(obj)):
        bad.append(f'{label}: one leaf, not the same object')


def groups(stmt):
    todo = [stmt]
    while todo:
        g = todo.pop()
        yield g
        todo.extend(t for t in g.tokens if t.is_group)


assert sqlparse_turbo.install()
native = sql.TokenList.__str__ is not original
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
texts = [open(p, encoding='utf-8').read() for p in sorted(glob.glob(os.path.join(root, 'bench', 'corpus', '*.sql')))]
texts += ['', 'select 1', 'select a from b; select c', '((((a))))', "select 'é½', \"x\" from t -- c\n"]
for text in texts:
    for stmt in sqlparse.parse(text):
        for g in groups(stmt):
            compare('corpus', g)

# trees a caller changed
base = 'select a.b, count(*) from t join u on t.id = u.id where x in (1, 2) group by a.b'


def fresh():
    return sqlparse.parse(base)[0]


class Mine(sql.Token):
    pass


changes = {
    'value replaced': lambda s: setattr(s.tokens[0], 'value', 'SELECT'),
    'value not a str': lambda s: setattr(s.tokens[0], 'value', 5),
    'value a str subclass': lambda s: setattr(s.tokens[0], 'value', type('S', (str,), {})('sel')),
    'token inserted': lambda s: s.tokens.insert(1, sql.Token(T.Comment, '/* x */')),
    'tokens removed': lambda s: s.tokens.clear(),
    'own token class': lambda s: s.tokens.insert(0, Mine(T.Other, 'm')),
    'is_group 1': lambda s: setattr(s.tokens[2], 'is_group', 1),
    'is_group on a leaf': lambda s: setattr(s.tokens[0], 'is_group', True),
    'is_group off on a group': lambda s: setattr(next(t for t in s.tokens if t.is_group), 'is_group', False),
    'tokens a tuple': lambda s: setattr(next(t for t in s.tokens if t.is_group), 'tokens', tuple(s.tokens[:2])),
    'value unset': lambda s: delattr(s.tokens[0], 'value'),
    'tokens unset': lambda s: delattr(next(t for t in s.tokens if t.is_group), 'tokens'),
    'not a token in tokens': lambda s: s.tokens.insert(0, 'select'),
    'cycle': lambda s: next(t for t in s.tokens if t.is_group).tokens.append(s),
}
for label, change in changes.items():
    s = fresh()
    change(s)
    compare(f'changed: {label}', s)


def nest(depth):
    leaf = sql.Token(T.Name, 'x')
    for _ in range(depth):
        leaf = sql.Parenthesis([sql.Token(T.Punctuation, '('), leaf, sql.Token(T.Punctuation, ')')])
    return sql.Statement([leaf])


for depth in (150, 199, 200, 201, 260, 2000):
    compare('deep', nest(depth))

# near the recursion limit


def at_depth(n, f):
    return f() if n <= 0 else at_depth(n - 1, f)


limit = sys.getrecursionlimit()
here = len(__import__('inspect').stack(0))
for left in (3, 10, 40, 120, 300):
    for depth in (5, 60):
        t = nest(depth)
        want = at_depth(limit - here - left, lambda t=t: outcome(original, t))
        got = at_depth(limit - here - left, lambda t=t: outcome(str, t))
        counts['near limit'] = counts.get('near limit', 0) + 1
        if want != got:
            bad.append(f'near the limit ({left} left, depth {depth}): {want} != {got}')

# a patched flatten is used, then the Rust again once it is put back
flatten = sql.TokenList.flatten
sql.TokenList.flatten = lambda self: iter([sql.Token(T.Other, 'patched')])
if str(fresh()) != 'patched':
    bad.append('a patched TokenList.flatten is not used')
sql.TokenList.flatten = flatten
compare('flatten put back', fresh())

# the stacks whose serializer the Rust runs, against sqlparse's own run
for text in texts[:120] + ['select 1; select 2;\n\nselect 3', 'select a from b\n']:
    for make in (filters.ReindentFilter, lambda: filters.AlignedIndentFilter(), lambda: filters.ReindentFilter(
            wrap_after=20, comma_first=True)):
        def run(stack_text=text, make=make):
            stack = FilterStack()
            stack.enable_grouping()
            stack.stmtprocess.append(make())
            stack.postprocess.append(filters.SerializerUnicode())
            f = stack.stmtprocess[0]
            out = list(stack.run(stack_text))
            state = {k: str(v) if isinstance(v, sql.Token) else v for k, v in vars(f).items()}
            return out, state
        got = run()
        sqlparse_turbo.uninstall()
        want = run()
        assert sqlparse_turbo.install()
        counts['serializer stacks'] = counts.get('serializer stacks', 0) + 1
        if want != got:
            bad.append(f'serializer stack {make}: {text[:60]!r}')
for text in texts[:60]:
    counts['parsestream'] = counts.get('parsestream', 0) + 1
    got = [str(x) for x in sqlparse.parsestream(io.StringIO(text))]
    sqlparse_turbo.uninstall()
    want = [str(x) for x in sqlparse.parsestream(io.StringIO(text))]
    assert sqlparse_turbo.install()
    if want != got:
        bad.append(f'parsestream: {text[:60]!r}')
sqlparse_turbo.uninstall()
if sql.TokenList.__str__ is not original:
    bad.append('uninstall() did not put TokenList.__str__ back')

print(f'CPython {sys.version.split()[0]}, str() in Rust: {native}; ' + ', '.join(f'{k} {v}' for k, v in counts.items()))
for b in bad[:20]:
    print('FAIL', b)
print(f'{len(bad)} differ' if bad else 'all equal')
sys.exit(1 if bad else 0)
