"""sqlparse against sqlparse_rs through the public calls, on the whole benchmark corpus.

    python -X utf8 bench/bench_package.py [--repeat N]

Best of N (default 3) per call and side; every result is also checked equal. Prints a Markdown table.
"""

import glob
import os
import sys
import time

import sqlparse
import sqlparse_rs

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPEAT = int(sys.argv[sys.argv.index('--repeat') + 1]) if '--repeat' in sys.argv else 3

CALLS = [
    ('`split(sql)`', 'split', {}),
    ("`format(sql, keyword_case='upper')`", 'format', {'keyword_case': 'upper'}),
    ("`format(sql, reindent=True, keyword_case='upper')`", 'format', {'reindent': True, 'keyword_case': 'upper'}),
    ('`format(sql, strip_comments=True)`', 'format', {'strip_comments': True}),
    ('`format(sql, reindent_aligned=True)`', 'format', {'reindent_aligned': True}),
    ("`format(sql, reindent=True, indent_columns=True, comma_first=True)`", 'format',
     {'reindent': True, 'indent_columns': True, 'comma_first': True}),
]


def best(fn, files, opts):
    times, out = [], None
    for _ in range(REPEAT):
        t = time.perf_counter()
        out = [fn(s, **opts) for s in files]
        times.append(time.perf_counter() - t)
    return min(times), out


def main():
    files = [open(p, encoding='utf-8').read() for p in sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', '*.sql')))]
    size = sum(len(f.encode('utf-8')) for f in files)
    print(f'{len(files)} files, {size / 1e6:.2f} MB, CPython {sys.version.split()[0]}, best of {REPEAT}\n')
    print('| Call | sqlparse | sqlparse_rs | Speed-up |')
    print('| --- | ---: | ---: | ---: |')
    for label, name, opts in CALLS:
        t_py, a = best(getattr(sqlparse, name), files, opts)
        t_rs, b = best(getattr(sqlparse_rs, name), files, opts)
        assert a == b, f'results differ for {label}'
        print(f'| {label} | {t_py:.2f} s | {t_rs:.3f} s | **{t_py / t_rs:.0f}x** |', flush=True)


if __name__ == '__main__':
    main()
