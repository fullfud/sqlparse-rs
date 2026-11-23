"""Compares tokenize() of the original and of the Rust module: every recorded tokenize case, the corpus, and
every case's SQL cut at random points (unclosed strings, comments and dollar quotes). Prints mismatches and exits 1
on any. PYD=path loads another build of the native module (e.g. the installed package's sqlparse_turbo._native);
the texts are rust-anything's recorded cases (rust/sqlparse-front/cases.jsonl, not committed: skipped when
missing) and the corpus (bench/gen_corpus.py)."""
import json, os, random, sys
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
import glob, importlib.util
sys.path.insert(0, os.path.join(ROOT, 'front'))
import sqlparse_front as py
_pyd = os.environ.get('PYD') or glob.glob(os.path.join(ROOT, 'rust', 'sqlparse-front', 'python', 'sqlparse_front', '_native*'))[0]
_spec = importlib.util.spec_from_file_location('_native', _pyd)
rs = importlib.util.module_from_spec(_spec); _spec.loader.exec_module(rs)

texts = []
_cases = os.path.join(ROOT, 'rust', 'sqlparse-front', 'cases.jsonl')
for line in open(_cases, encoding='utf-8') if os.path.exists(_cases) else ():
    d = json.loads(line)
    if d['fn'] in ('tokenize', 'split', 'format') and isinstance(d['args'][0], str):
        texts.append(d['args'][0])
texts += [open(p, encoding='utf-8').read() for p in sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', '*.sql')))]
texts = list(dict.fromkeys(texts))
r = random.Random(1)
cuts = []
for t in texts:
    for _ in range(3):
        if len(t) > 2:
            a = r.randrange(len(t)); b = r.randrange(a, len(t) + 1)
            cuts.append(t[a:b])
bad = 0
for t in texts + cuts:
    a, b = py.tokenize(t), [tuple(x) for x in rs.tokenize(t)]
    if a != b:
        bad += 1
        if bad <= 5:
            i = next(i for i in range(min(len(a), len(b))) if a[i] != b[i]) if any(x != y for x, y in zip(a, b)) else min(len(a), len(b))
            print('MISMATCH at token', i, 'py', a[i:i+3], 'rs', b[i:i+3], 'text', repr(t[:120]))
print(f'{len(texts) + len(cuts)} texts ({len(texts)} recorded or corpus, {len(cuts)} cut), mismatches {bad}')

# random text from pieces where Python and Rust regex semantics part: marks, numerics, case folding, delimiters
PIECES = ['select', 'SELECT', 'from', 'end if', 'END\tLOOP', 'left  outer join', 'not null', 'nulls first', 'x.y',
          '.5', '1.', '123abc', '-0x1F', '1e-5', '1.5E3x', "'it''s'", "'a\'b'", '"q""q"', '`b``t`', '´n´', '[a b]',
          'a[1]', '$1', ':name', '?', '%s', '%(id)s', '$$', '$tag$', '$TAG$', '$a$x$a$', '/*', '*/', '/*+', '--',
          '-- c\n', '# x', '#tmp', '@v', '##g', '\cmd', ':=', '::', '->>', '#>', '@>', '<@', '?|', '?&', '<=>', '!=',
          'à', 'É', 'ſelect', 'KelvinK', 'straße', 'İ', 'ı', '́', 'é', '½', '²', '٣',
          'Ⅷ', '×', '÷', 'ǅ', '　', ' ', '\x1c', '\x85', '\xa0', '\r\n', '\r', '\n', ' ',
          '\t', '(', ')', ',', ';', 'at time zone \'UTC\'', 'with\' time zone \'x\'', 'GO 5', 'lateral view explode',
          'double precision', 'group  by', 'union all', 'create or replace', 'not ilike', 'regexp binary', '😀', '\x00']
r2 = random.Random(7)
fuzz_bad = 0
N = 30000
for _ in range(N):
    t = ''.join(r2.choice(PIECES) for _ in range(r2.randint(1, 12)))
    a, b = py.tokenize(t), [tuple(x) for x in rs.tokenize(t)]
    if a != b:
        fuzz_bad += 1
        if fuzz_bad <= 8:
            print('FUZZ MISMATCH', repr(t), '\n   py', a, '\n   rs', b)
print(f'fuzz: {N} random texts, mismatches {fuzz_bad}')
sys.exit(1 if bad or fuzz_bad else 0)
