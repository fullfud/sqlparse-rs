"""tokenize() on the whole corpus: the original against the Rust module (bridge included), best of 3."""
import glob, importlib.util, os, sys, time
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, 'front'))
import sqlparse_front as py
_pyd = glob.glob(os.path.join(ROOT, 'rust', 'sqlparse-front', 'python', 'sqlparse_front', '_native*'))[0]
_spec = importlib.util.spec_from_file_location('_native', _pyd)
rs = importlib.util.module_from_spec(_spec); _spec.loader.exec_module(rs)
files = [open(p, encoding='utf-8').read() for p in sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', '*.sql')))]
for name, f in (('python', py.tokenize), ('rust', rs.tokenize)):
    best = min((lambda: (lambda t: [f(s) for s in files] and time.perf_counter() - t)(time.perf_counter()))() for _ in range(3))
    print(f'{name:6s} {best:6.2f} s')
