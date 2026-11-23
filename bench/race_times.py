"""Per-file times for the README's race animation and speed chart: the 400 query files of the benchmark corpus,
sqlparse against sqlparse_turbo, best of N per file and side, every result checked equal.

    python -X utf8 bench/race_times.py out.json [--repeat N]
"""

import glob
import hashlib
import json
import os
import platform
import sys
import time

import sqlparse
import sqlparse_turbo

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPEAT = int(sys.argv[sys.argv.index('--repeat') + 1]) if '--repeat' in sys.argv else 3

CALLS = {
    'split': lambda m, s: m.split(s),
    'format_upper': lambda m, s: m.format(s, keyword_case='upper'),
    'format_reindent': lambda m, s: m.format(s, reindent=True, keyword_case='upper'),
    'parse': lambda m, s: [str(t) for t in m.parse(s)],
}


def per_file(call, module, files):
    times, outs = [], []
    for s in files:
        best = None
        for _ in range(REPEAT):
            t = time.perf_counter()
            out = call(module, s)
            d = time.perf_counter() - t
            best = d if best is None else min(best, d)
        times.append(best)
        outs.append(out)
    return times, outs


def main():
    paths = sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', 'q*.sql')))
    files = [open(p, encoding='utf-8').read() for p in paths]
    result = {
        'files': len(files), 'bytes': sum(len(f.encode()) for f in files),
        'python': platform.python_version(), 'sqlparse': sqlparse.__version__, 'calls': {},
        'sample': files[7],
    }
    for name, call in CALLS.items():
        a, out_a = per_file(call, sqlparse, files)
        b, out_b = per_file(call, sqlparse_turbo, files)
        if out_a != out_b:
            sys.exit(f'{name}: results differ')
        digest = hashlib.sha256(json.dumps(out_a).encode()).hexdigest()
        result['calls'][name] = {'sqlparse': a, 'turbo': b, 'sha256': digest}
        print(f'{name}: sqlparse {sum(a):.3f} s, turbo {sum(b):.4f} s, {sum(a) / sum(b):.1f}x, sha {digest[:12]}')
    result['sample_formatted'] = sqlparse_turbo.format(files[7], reindent=True, keyword_case='upper')
    with open(sys.argv[1], 'w', encoding='utf-8') as f:
        json.dump(result, f)


main()
