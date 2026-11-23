"""The package with a sqlparse other than 0.6.1: it imports, install() refuses, and every call is answered by that
sqlparse itself (same results, same errors), with nothing reaching the Rust.

    pip install sqlparse==0.5.3 && python tools/check_other_sqlparse.py
"""

import glob
import os
import sys
import warnings

import sqlparse

with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter('always')
    import sqlparse_turbo
from sqlparse_turbo import _native

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

if sqlparse.__version__ == sqlparse_turbo.PORTED_VERSION:
    sys.exit(f'sqlparse {sqlparse.__version__} is the ported version: install another one to run this check')

reached = []
for name in ('format', 'split', 'split_many', 'parse', 'lex'):
    real = getattr(_native, name)
    setattr(_native, name, lambda *a, _n=name, _r=real, **k: (reached.append(_n), _r(*a, **k))[1])

texts = [
    'select a, b as c from t where x = 1 and y in (select 2) order by a; insert into u values (1, \'x\')',
    'create table t (id int primary key, name varchar(20)); -- comment\nupdate t set name = null where id = 1;',
    'select case when a > 1 then \'big\' else \'small\' end from t left join u on t.id = u.id group by 1',
    'BEGIN select 1; END; select f(x, y) over (partition by z) from w',
    'sélect ñ from 表',
]
for path in sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', '*.sql')))[:40]:
    with open(path, encoding='utf-8') as f:
        texts.append(f.read())


def outcome(f):
    try:
        return 'ok', f()
    except Exception as e:  # the same error class and message as sqlparse's
        return type(e), str(e)


bad = 0
options = [{}, {'reindent': True, 'keyword_case': 'upper'}, {'strip_comments': True, 'reindent_aligned': True},
           {'reindent': 'yes'}]
for text in texts:
    pairs = [(f'format {o}', lambda o=o: sqlparse.format(text, **o), lambda o=o: sqlparse_turbo.format(text, **o))
             for o in options]
    pairs += [
        ('split', lambda: sqlparse.split(text), lambda: sqlparse_turbo.split(text)),
        ('parse', lambda: [str(s) for s in sqlparse.parse(text)], lambda: [str(s) for s in sqlparse_turbo.parse(text)]),
    ]
    for what, a, b in pairs:
        if outcome(a) != outcome(b):
            print('MISMATCH', what, repr(text[:60]))
            bad += 1
if sqlparse_turbo.split_many(texts) != [sqlparse.split(t) for t in texts]:
    print('MISMATCH split_many')
    bad += 1
if sqlparse_turbo.ACTIVE or not any(issubclass(w.category, RuntimeWarning) and 'only with sqlparse' in str(w.message)
                                    for w in caught):
    print('no warning at import, or ACTIVE is True')
    bad += 1
if sqlparse_turbo.install() is not False or sqlparse.format is sqlparse_turbo.format:
    print('install() did not refuse')
    bad += 1
if reached:
    print('REACHED RUST:', sorted(set(reached)))
    bad += 1
print(f'sqlparse {sqlparse.__version__}: {len(texts)} texts, every call answered by sqlparse; mismatches {bad}')
sys.exit(1 if bad else 0)
