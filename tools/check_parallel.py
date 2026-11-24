"""Every check of the package, side by side on all cores: what CI runs for each OS x Python.

    python -X utf8 tools/check_parallel.py [--whole] [--core] [--jobs N]

Without --whole the package check runs its quick cut (every 8th corpus file and code point chunk); --whole runs
all of it. --core adds the Rust core against the front door (check_tokenize.py, check_tree.py quick). The long
checks are cut in shares (CHECK_SHARD=i/n, see check_package.py and check_recursion.py); the longest tasks start
first, each task's output is printed whole when it ends, and the exit code is 1 if any task failed. Needs the
corpus (bench/gen_corpus.py), Django and django-debug-toolbar installed.
"""
import concurrent.futures
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
args = sys.argv[1:]
WHOLE = '--whole' in args
CORE = '--core' in args
JOBS = int(args[args.index('--jobs') + 1]) if '--jobs' in args else (os.cpu_count() or 2)


def tool(name):
    return os.path.join(ROOT, 'tools', name)


def task(label, script, *script_args, weight, shards=1, env=None):
    """`shards` tasks running `script` with CHECK_SHARD=i/shards, each `weight / shards` (seconds, roughly)."""
    out = []
    for i in range(shards):
        e = dict(env or {})
        if shards > 1:
            e['CHECK_SHARD'] = f'{i}/{shards}'
        out.append((f'{label} {i + 1}/{shards}' if shards > 1 else label,
                    [sys.executable, '-X', 'utf8', tool(script), *script_args], e, weight / shards))
    return out


# rough seconds on CPython 3.10, the slowest, on a CI runner (the whole package check: unicode 1460, read 512,
# stacks 498, lexswap 170, corpus 90)
SMALL = ['fallbacks', 'inert', 'lexer', 'installed', 'cli', 'many', 'patches']
quick = [] if WHOLE else ['--quick']
scale = 1 if WHOLE else 1 / 8
tasks = []
tasks += task('package unicode', 'check_package.py', *quick, 'unicode', weight=1460 * scale, shards=8 if WHOLE else 2)
tasks += task('package read', 'check_package.py', *quick, 'read', weight=512 * scale, shards=4 if WHOLE else 1)
tasks += task('package stacks', 'check_package.py', *quick, 'stacks', weight=498 * scale, shards=3 if WHOLE else 1)
tasks += task('package lexswap', 'check_package.py', *quick, 'lexswap', weight=170 * scale, shards=2 if WHOLE else 1)
tasks += task('package corpus', 'check_package.py', *quick, 'corpus', weight=90 * scale)
tasks += task('package small parts', 'check_package.py', *quick, *SMALL, weight=10)
tasks += task('recursion', 'check_recursion.py', 'package', weight=210, shards=4)
tasks += task('django-debug-toolbar', 'check_toolbar.py', weight=70)
tasks += task('str() and serializer stacks', 'check_str.py', weight=30)
tasks += task("sqlparse's test suite", 'run_upstream_package.py', weight=10)
tasks += task('Django app', 'check_django.py', weight=5)
if CORE:
    import sqlparse_turbo._native as native

    pyd = {'PYD': native.__file__}
    tasks += task('core: tokenize', 'check_tokenize.py', weight=40, env=pyd)
    tasks += task('core: trees', 'check_tree.py', 'quick', weight=45, env=pyd)
tasks.sort(key=lambda t: -t[3])


def run(t):
    label, cmd, env, _ = t
    start = time.perf_counter()
    p = subprocess.run(cmd, cwd=ROOT, env={**os.environ, **env}, capture_output=True, text=True,
                       encoding='utf-8', errors='replace')
    return label, p.returncode, p.stdout + p.stderr, time.perf_counter() - start


start = time.perf_counter()
failed = []
print(f'{len(tasks)} tasks on {JOBS} workers ({"whole" if WHOLE else "quick"} package check'
      f'{", core" if CORE else ""})', flush=True)
with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
    futures = [pool.submit(run, t) for t in tasks]  # submitted longest first: started in that order
    for future in concurrent.futures.as_completed(futures):
        label, code, output, took = future.result()
        status = 'ok' if code == 0 else f'FAILED (exit {code})'
        print(f'\n===== {label}: {status}, {took:.0f} s', flush=True)
        print(output.rstrip(), flush=True)
        if code:
            failed.append(label)
print(f'\n{len(tasks) - len(failed)} of {len(tasks)} tasks passed in {time.perf_counter() - start:.0f} s'
      + (f'; failed: {", ".join(failed)}' if failed else ''))
sys.exit(1 if failed else 0)
