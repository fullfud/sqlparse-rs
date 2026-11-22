"""tokenize() of the original and the Rust lexer for every Unicode scalar value, alone and in contexts that hit
each kind of rule: a word, a number, a dollar tag, a name before '(' or '.', a placeholder, a bracket name."""
import glob, importlib.util, os, sys, time
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, 'front'))
import sqlparse_front as py
pyd = glob.glob(os.path.join(ROOT, 'rust', 'sqlparse-front', 'python', 'sqlparse_front', '_native*'))[0]
spec = importlib.util.spec_from_file_location('_native', pyd); rs = importlib.util.module_from_spec(spec); spec.loader.exec_module(rs)
CONTEXTS = ['{}', 'a{}b', '1{}', '${}$x${}$', 'x{}(', '{}x.y', ':{}', '[{}]', 'end{}', "'{}'", '#{}x']
t0 = time.perf_counter(); bad = 0; n = 0
for cp in range(0x110000):
    if 0xD800 <= cp <= 0xDFFF:
        continue
    c = chr(cp)
    for ctx in CONTEXTS:
        s = ctx.format(c, c)
        n += 1
        if py.tokenize(s) != [tuple(x) for x in rs.tokenize(s)]:
            bad += 1
            if bad <= 10:
                print('MISMATCH', hex(cp), repr(s), py.tokenize(s), rs.tokenize(s))
print(f'{n} texts ({len(CONTEXTS)} contexts x every scalar value), mismatches {bad}, {time.perf_counter() - t0:.0f} s')
