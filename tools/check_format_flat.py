"""format() without grouping options, original against the Rust port: case filters over every Unicode scalar
value (alone, inside names, around a capital sigma), string truncation, python/php output, bad option values."""
import glob, importlib.util, json, os, random, sys, time
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, 'front'))
import sqlparse_front as py
pyd = os.environ.get('PYD') or glob.glob(os.path.join(ROOT, 'rust', 'sqlparse-front', 'python', 'sqlparse_front', '_native*'))[0]
spec = importlib.util.spec_from_file_location('_native', pyd); rs = importlib.util.module_from_spec(spec); spec.loader.exec_module(rs)
NAMES = ['keyword_case', 'identifier_case', 'strip_comments', 'use_space_around_operators', 'strip_whitespace',
         'truncate_strings', 'truncate_char', 'reindent', 'reindent_aligned', 'indent_columns', 'indent_after_first',
         'indent_tabs', 'indent_width', 'wrap_after', 'comma_first', 'compact', 'right_margin', 'output_format']
DEFAULTS = [None, None, False, False, False, None, '[...]', False, False, False, False, False, 2, 0, False, False, None, None]
bad = 0
def run(f, text, opts):
    try:
        return ('ok', f(text, **opts)) if f is py.format else ('ok', f(text, *[opts.get(n, d) for n, d in zip(NAMES, DEFAULTS)]))
    except Exception as e:
        return ('err', str(e))
def check(text, **opts):
    global bad
    a, b = run(py.format, text, opts), run(rs.format, text, opts)
    if a[0] == 'err' and b[0] == 'err' and b[1].startswith('SQLParseError: '):
        # the bridge has no SQLParseError class: it raises ValueError('SQLParseError: ...')
        b = ('err', b[1].split(': ', 1)[1])
    if a != b:
        bad += 1
        if bad <= 8:
            print('MISMATCH', opts, repr(text[:100]), '\n  py', repr(a)[:200], '\n  rs', repr(b)[:200])
t0 = time.perf_counter()
CASES = [{'keyword_case': c} for c in ('upper', 'lower', 'capitalize')] + [{'identifier_case': c} for c in ('upper', 'lower', 'capitalize')]
CTX = ['select {} from x', 'select a{}b, "q{}" from t', 'select {}Σ, xΣ{}, {}Σa from Σ{}', 'select straße{} as ǆ{}']
n = 0
for cp in range(0x110000):
    if 0xD800 <= cp <= 0xDFFF:
        continue
    c = chr(cp)
    for ctx in CTX:
        t = ctx.replace('{}', c)
        for o in CASES:
            check(t, **o); n += 1
print(f'{n} case-filter texts done')
texts = [open(p, encoding='utf-8').read() for p in sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', 'q*.sql')))]
r = random.Random(5)
for t in texts:
    for o in CASES + [{'truncate_strings': r.randint(2, 12), 'truncate_char': r.choice(['[...]', '…', '', 'Σ'])},
                      {'output_format': 'python'}, {'output_format': 'php'},
                      {'output_format': 'python', 'keyword_case': 'upper', 'truncate_strings': 3}]:
        check(t, **o)
        check(t.replace('\n', '\r\n'), **o)
        check(t + "\n'unclosed \\n x", **o)
for v in ['UPPER', 'title', '', 'ﬁ', "it's", 'a"b\'c', '\x00\t\n\x7f\x85​\U0001F600']:
    check('select 1', keyword_case=v); check('select 1', identifier_case=v); check('select 1', output_format=v)
for w in [-5, 0, 1, 2]:
    check("select 'abcdef'", truncate_strings=w)
for w in [-1, 0]:
    check('select 1', indent_width=w)
check('select 1', wrap_after=-1); check('select 1', right_margin=9)
print(f'mismatches {bad}, {time.perf_counter() - t0:.0f} s')
