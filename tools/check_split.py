"""split() of the original and the Rust port, both strip_semicolon values: recorded texts, random cuts,
random block-structured scripts, and every Unicode scalar value where the splitter upper-cases, splits words
or strips."""
import glob, importlib.util, json, os, random, sys, time
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, 'front'))
import sqlparse
pyd = os.environ.get('PYD') or glob.glob(os.path.join(ROOT, 'rust', 'sqlparse-front', 'python', 'sqlparse_front', '_native*'))[0]
spec = importlib.util.spec_from_file_location('_native', pyd); rs = importlib.util.module_from_spec(spec); spec.loader.exec_module(rs)

bad = 0
def check(t):
    global bad
    for strip in (False, True):
        a = sqlparse.split(t, strip_semicolon=strip)
        b = rs.split(t, strip)
        if a != b:
            bad += 1
            if bad <= 8:
                print('MISMATCH', strip, repr(t[:160]), '\n  py', a[:4], '\n  rs', b[:4])

texts = [open(p, encoding='utf-8').read() for p in sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', '*.sql')))]
for line in open(os.path.join(ROOT, 'rust', 'sqlparse-front', 'cases.jsonl'), encoding='utf-8'):
    d = json.loads(line)
    if isinstance(d['args'][0], str):
        texts.append(d['args'][0])
texts = list(dict.fromkeys(texts))
r = random.Random(3)
cuts = [t[a:b] for t in texts for a, b in [sorted((r.randrange(len(t) + 1), r.randrange(len(t) + 1))) for _ in range(3)]]
t0 = time.perf_counter()
for t in texts + cuts:
    check(t)
print(f'{len(texts)} recorded + {len(cuts)} cut texts done')

PIECES = ['begin', 'BEGIN', 'Begin', 'end', 'END', 'end if', 'END IF', 'end loop', 'END LOOP', 'end while', 'end for',
          'end case', 'if', 'IF', 'case', 'loop', 'for', 'while', 'do', 'declare', 'create', 'CREATE OR REPLACE',
          'create function f() as', 'transaction', 'work', 'tran', 'deferred', 'immediate', 'exclusive', 'distributed',
          'commit', 'select 1', 'from t', ';', ';', ' ;', '(', ')', ' ', '\n', '\t', '\x1c', '\x1f', '\x85', '\xa0',
          '　', '-- c\n', '/* c */', 'GO', 'go', 'GO 5', 'GO\x1c5', 'GO　5', "'x;y'", '$$a;b$$', '"a;b"',
          'then', 'else', 'return', 'x := 1', 'ſ', 'ß', 'İ', 'ı', 'ﬁ']
for _ in range(40000):
    check(' '.join(r.choice(PIECES) for _ in range(r.randint(1, 25))))
print('random block scripts done')

CONTEXTS = ['{}begin x; end;', 'create {}; begin; end', 'begin {} ; end; x;', 'select 1;{}select 2',
            'GO{}5\nx', '{}select 1;{}', 'begin transaction{}; x;', 'end{}if; x', 'declare{} x; begin end; y']
n = 0
for cp in range(0x110000):
    if 0xD800 <= cp <= 0xDFFF:
        continue
    c = chr(cp)
    for ctx in CONTEXTS:
        check(ctx.format(c, c)); n += 1
print(f'{n} code point texts done')
print(f'mismatches {bad}, {time.perf_counter() - t0:.0f} s')
sys.exit(1 if bad else 0)
