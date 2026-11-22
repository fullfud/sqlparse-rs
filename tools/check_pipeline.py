"""Every recorded and corpus text through split, format (12 option sets) and parse, with the original lexer
and with the Rust one: outputs and whole parse trees must be identical. Also times both."""
import glob, json, os, sys, time
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path[:0] = [os.path.join(ROOT, 'front'), os.path.join(ROOT, 'bench')]
import sqlparse
import rs_lexer
from make_inputs import FORMATS

texts = [open(p, encoding='utf-8').read() for p in sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', '*.sql')))]
for line in open(os.path.join(ROOT, 'rust', 'sqlparse-front', 'cases.jsonl'), encoding='utf-8'):
    d = json.loads(line)
    if isinstance(d['args'][0], str):
        texts.append(d['args'][0])
texts = list(dict.fromkeys(texts))

def tree(node):
    if node.is_group:
        return (type(node).__name__, [tree(t) for t in node.tokens])
    return (str(node.ttype), node.value)

def run(text):
    out = [sqlparse.split(text), sqlparse.split(text, strip_semicolon=True)]
    for opts in FORMATS:
        try:
            out.append(sqlparse.format(text, **opts))
        except Exception as e:
            out.append(f'{type(e).__name__}: {e}')
    out.append([tree(s) for s in sqlparse.parse(text)])
    return out

t = time.perf_counter(); expected = [run(x) for x in texts]; t_py = time.perf_counter() - t
rs_lexer.install()
t = time.perf_counter(); got = [run(x) for x in texts]; t_rs = time.perf_counter() - t
bad = [i for i in range(len(texts)) if expected[i] != got[i]]
for i in bad[:5]:
    print('MISMATCH', repr(texts[i][:100]))
print(f'{len(texts)} texts x (2 split + {len(FORMATS)} format + parse tree): mismatches {len(bad)}')
print(f'time: original lexer {t_py:.1f} s, Rust lexer {t_rs:.1f} s')
