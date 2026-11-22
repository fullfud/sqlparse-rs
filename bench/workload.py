"""The workload the port is profiled and timed on: what callers of sqlparse do with real SQL.

    python bench/workload.py [corpus dir] [--only format|split|parse]

For every corpus file: sqlparse.split(), sqlparse.format() with the options people use (reindent + upper-case
keywords; strip comments), and sqlparse.parse() walked to the last token. Prints the time of each part.
"""

import os
import sys
import time

import sqlparse


def run(files, only=None):
    times = {}

    def timed(name, fn):
        if only and only != name:
            return
        t = time.perf_counter()
        for sql in files:
            fn(sql)
        times[name] = time.perf_counter() - t

    timed('split', sqlparse.split)
    timed('format', lambda s: (sqlparse.format(s, reindent=True, keyword_case='upper'),
                               sqlparse.format(s, strip_comments=True)))
    timed('parse', lambda s: [list(st.flatten()) for st in sqlparse.parse(s)])
    return times


def main():
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    only = sys.argv[sys.argv.index('--only') + 1] if '--only' in sys.argv else None
    if only:
        args = [a for a in args if a != only]
    corpus = args[0] if args else os.path.join(os.path.dirname(os.path.abspath(__file__)), 'corpus')
    files = []
    for name in sorted(os.listdir(corpus)):
        if name.endswith('.sql'):
            with open(os.path.join(corpus, name), encoding='utf-8') as f:
                files.append(f.read())
    times = run(files, only)
    size = sum(len(s) for s in files) / 1e6
    print(f'{len(files)} files, {size:.1f} MB')
    for k, v in times.items():
        print(f'{k:7s} {v:7.2f} s')
    print(f'total   {sum(times.values()):7.2f} s')


if __name__ == '__main__':
    main()
