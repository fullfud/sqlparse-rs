"""Draws the README's and the docs' static pictures (docs/assets/*.svg) from numbers measured for them.

    python tools/make_cards.py

The race animation and the speed chart come from tools/make_visuals.py, which needs bench/race_times.py's
times. Each number here says where it was measured; change it there first.
"""

import html
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, 'docs', 'assets')

BG, CARD, BORDER = '#0d0e12', '#16171d', '#2a2c34'
TEXT, DIM, FAINT = '#ecedf0', '#9699a2', '#5f626c'
GREY, ACCENT, GREEN, BLUE, PURPLE, RED = '#8c8f98', '#ff8a3d', '#7ed385', '#6eaaff', '#be96ff', '#e0625a'
TRACK = '#22242c'
SANS = 'Segoe UI, -apple-system, Helvetica, Arial, sans-serif'
MONO = 'Cascadia Mono, Consolas, Menlo, monospace'
LEFT, RIGHT = 28, 832


class Svg:
    def __init__(self, w, h, title, sub=None):
        self.w, self.h = w, h
        self.defs = []
        self.parts = [f'<rect width="{w}" height="{h}" rx="14" fill="{BG}"/>']
        self.text(LEFT, 38, title, size=20, weight=700)
        if sub:
            self.text(LEFT, 60, sub, fill=DIM, size=14)

    def text(self, x, y, s, fill=TEXT, size=14, weight=None, anchor=None, mono=False):
        attrs = [f'x="{x:g}"', f'y="{y:g}"', f'fill="{fill}"', f'font-size="{size}"']
        if weight:
            attrs.append(f'font-weight="{weight}"')
        if anchor:
            attrs.append(f'text-anchor="{anchor}"')
        if mono:
            attrs.append(f'font-family="{MONO}"')
        self.parts.append(f'<text {" ".join(attrs)}>{html.escape(s, quote=False)}</text>')

    def rect(self, x, y, w, h, fill, rx=7, stroke=None, dash=False):
        extra = f' stroke="{stroke}" stroke-width="1.2"' if stroke else ''
        extra += ' stroke-dasharray="5 4"' if dash else ''
        self.parts.append(f'<rect x="{x:g}" y="{y:g}" width="{w:.1f}" height="{h:g}" rx="{rx}" fill="{fill}"{extra}/>')

    def card(self, x, y, w, h, stroke=BORDER, dash=False):
        self.rect(x, y, w, h, CARD, rx=10, stroke=stroke, dash=dash)

    def tick(self, x, y, colour=GREEN):
        """A check mark whose box is (x, y)-(x+12, y+10)."""
        self.parts.append(f'<polyline points="{x},{y + 5} {x + 4},{y + 9} {x + 12},{y}" fill="none" '
                          f'stroke="{colour}" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>')

    def arrow(self, d, colour, dash=False):
        mid = f'm{len(self.defs)}'
        self.defs.append(f'<marker id="{mid}" viewBox="0 0 10 10" refX="8" refY="5" markerUnits="userSpaceOnUse" '
                         f'markerWidth="10" markerHeight="10" orient="auto"><path d="M0 0 L10 5 L0 10 z" '
                         f'fill="{colour}"/></marker>')
        dashes = ' stroke-dasharray="5 4"' if dash else ''
        self.parts.append(f'<path d="{d}" fill="none" stroke="{colour}" stroke-width="2"{dashes} '
                          f'marker-end="url(#{mid})"/>')

    def save(self, name):
        head = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{self.w}" height="{self.h}" '
                f'viewBox="0 0 {self.w} {self.h}" font-family="{SANS}">')
        body = ([head] + (['<defs>'] + self.defs + ['</defs>'] if self.defs else []) + self.parts + ['</svg>'])
        path = os.path.join(OUT, name)
        with open(path, 'w', encoding='utf-8', newline='\n') as f:
            f.write('\n'.join(body) + '\n')
        print(path)


def how_it_works():
    s = Svg(860, 344, 'How a call runs',
            "every call is checked first; whichever side answers, the answer is sqlparse's")
    top, h = 84, 212
    mid_y = top + h / 2
    # the caller
    s.card(LEFT, top, 200, h)
    s.text(48, top + 30, 'Your code', size=17, weight=700)
    s.text(48, top + 50, 'Django, tools, scripts', fill=DIM, size=13)
    for i, line in enumerate(['sqlparse.format(sql)', 'sqlparse.split(sql)', 'sqlparse.parse(sql)',
                              'parsestream(stream)']):
        s.text(48, top + 86 + i * 24, line, size=13, mono=True)
    s.text(48, top + h - 16, 'with install() in place', fill=FAINT, size=12)
    s.arrow(f'M{LEFT + 204} {mid_y} H{258}', DIM)
    # the checks
    s.card(264, top, 250, h)
    s.text(284, top + 30, 'sqlparse_turbo checks', size=17, weight=700)
    checks = ['sqlparse is 0.6.1', 'str, bytes or a text stream', 'options of the exact types',
              'sqlparse not monkeypatched', 'default lexer unchanged', 'no Unicode drift in the text']
    for i, c in enumerate(checks):
        y = top + 62 + i * 24
        s.tick(286, y - 9)
        s.text(306, y, c, size=13)
    # the two sides
    box_h = 98
    s.card(570, top, RIGHT - 570, box_h, stroke=ACCENT)
    s.text(590, top + 24, 'every check passes', fill=DIM, size=12)
    s.text(RIGHT - 20, top + 24, '30-150x faster', fill=ACCENT, size=12, weight=700, anchor='end')
    s.text(590, top + 52, 'the Rust port', fill=ACCENT, size=19, weight=700)
    s.text(590, top + 78, 'lexer, splitter, grouping, filters', fill=DIM, size=13)
    low = top + h - box_h
    s.card(570, low, RIGHT - 570, box_h, dash=True)
    s.text(590, low + 24, 'any check fails', fill=DIM, size=12)
    s.text(590, low + 52, 'sqlparse 0.6.1 itself', size=19, weight=700)
    s.text(590, low + 78, 'unchanged, at its own speed', fill=DIM, size=13)
    s.arrow(f'M516 {mid_y} H540 V{top + box_h / 2} H566', ACCENT)
    s.arrow(f'M540 {mid_y} V{low + box_h / 2} H566', GREY, dash=True)
    s.text(LEFT, 326, 'Same output, byte for byte, and the same errors, whichever side answers.', size=14)
    s.save('how-it-works.svg')


def proof():
    # The README's check table, docs/verification.md: every number is a run of the tools there.
    tiles = [
        ('7,842 / 7,842', "recorded calls give sqlparse's", 'answer, errors included'),
        ('537 passed', "sqlparse's own test suite,", 'run through the package'),
        ('260,837 trees', 'compared node by node with', "sqlparse's: 0 differ"),
        ('26.7 million', 'texts over every Unicode code', 'point: 0 differ'),
        ('25 / 25', 'platform × Python jobs in CI', 'test the wheel users install'),
        ('same depth', 'the recursion error comes at', "sqlparse's nesting, 3.10-3.14"),
    ]
    s = Svg(860, 344, 'Checked against sqlparse itself',
            'the port has to give the answer sqlparse gives, on everything it is shown')
    gap, th = 16, 100
    tw = (RIGHT - LEFT - 2 * gap) / 3
    for i, (big, l1, l2) in enumerate(tiles):
        x = LEFT + (i % 3) * (tw + gap)
        y = 82 + (i // 3) * (th + gap)
        s.card(x, y, tw, th)
        s.text(x + 20, y + 42, big, fill=ACCENT, size=27, weight=700)
        s.text(x + 20, y + 66, l1, fill=DIM, size=13)
        s.text(x + 20, y + 84, l2, fill=DIM, size=13)
    s.text(LEFT, 330, 'Every check is a script in tools/; the whole list is in docs/verification.md', fill=FAINT,
           size=12)
    s.save('proof.svg')


def platforms():
    # .github/workflows/ci.yml: the wheels job and the test matrix (5 runners x CPython 3.10-3.14).
    rows = [('Linux x86_64', 'manylinux2014'), ('Linux aarch64', 'manylinux2014'),
            ('Windows x64', 'win_amd64'), ('macOS arm64', 'universal2 wheel'), ('macOS Intel', 'universal2 wheel')]
    pythons = ['3.10', '3.11', '3.12', '3.13', '3.14']
    s = Svg(860, 400, 'Wheels and CI',
            'one abi3 wheel per platform for CPython 3.10 and later · every cell is a CI job testing that wheel')
    x0, cw, y0, rh = 250, 116, 116, 44
    for j, p in enumerate(pythons):
        s.text(x0 + j * cw + cw / 2, 100, f'CPython {p}', fill=DIM, size=13, anchor='middle')
    for i, (name, sub) in enumerate(rows):
        y = y0 + i * rh
        if i:
            s.rect(LEFT, y, RIGHT - LEFT, 1, BORDER, rx=0)
        s.text(LEFT, y + 21, name, size=15, weight=600)
        s.text(LEFT, y + 37, sub, fill=FAINT, size=12)
        for j in range(len(pythons)):
            cx = x0 + j * cw + cw / 2
            s.rect(cx - 38, y + 10, 76, 26, '#16271b', rx=13, stroke='#2f6b3f')
            s.tick(cx - 22, y + 18)
            s.text(cx + 8, y + 28, 'pass', fill=GREEN, size=13, anchor='middle')
    s.text(LEFT, y0 + 5 * rh + 26, "Each job runs sqlparse's test suite, the recursion check, the Django check and "
           'the package check.', fill=DIM, size=13)
    s.text(LEFT, y0 + 5 * rh + 48, 'No wheels and no tests for musl (Alpine), Windows on ARM, PyPy or free-threaded '
           'CPython: pip builds from source there.', fill=FAINT, size=12)
    s.save('platforms.svg')


def calls():
    # docs/benchmarks.md, "Other ways into sqlparse": 400 query files, CPython 3.13, best of 3, with install().
    rows = [
        ("format(sql.encode(), keyword_case='upper')", 'bytes in, 400 files', 87, '1.08 s → 0.012 s'),
        ('format(sql, reindent=True)', 'one short ORM query', 71, '471 µs → 6.7 µs'),
        ('split(sql)', 'one short ORM query', 67, '117 µs → 1.8 µs'),
        ('FilterStack.run, AlignedIndentFilter', 'own stack, 400 files', 66, '3.55 s → 0.054 s'),
        ("django-debug-toolbar's SQL panel", 'its own filter stacks', 62, '4.89 s → 0.079 s'),
        ('[str(s) for s in parsestream(stream)]', '400 files', 23, '3.00 s → 0.133 s'),
    ]
    top, rh = 84, 60
    s = Svg(860, top + rh * len(rows) + 34, 'Every way into sqlparse',
            "with install(), CPython 3.13, best of 3, every result equal to sqlparse's")
    full = 470
    for i, (label, what, x, times) in enumerate(rows):
        y = top + i * rh
        s.text(LEFT, y + 16, label, size=14, mono=True)
        s.text(RIGHT, y + 16, times, fill=DIM, size=13, anchor='end')
        s.rect(LEFT, y + 26, full, 14, TRACK)
        w = max(14, full * x / rows[0][2])
        s.rect(LEFT, y + 26, w, 14, ACCENT)
        s.text(LEFT + full + 14, y + 38, f'{x:g}x faster', fill=ACCENT, size=14, weight=700)
        s.text(LEFT + full + 114, y + 38, what, fill=FAINT, size=12)
    s.text(LEFT, s.h - 16, 'How each was measured: docs/benchmarks.md', fill=FAINT, size=12)
    s.save('calls.svg')


def memory():
    # 0.1.5's README: iterating parsestream over 100,000 INSERT statements, peak of the whole process.
    rows = [('sqlparse 0.6.1', 56, GREY), ('sqlparse-turbo 0.1.4', 410, RED), ('sqlparse-turbo 0.1.5', 59, ACCENT)]
    s = Svg(860, 236, 'Memory: parsestream over 100,000 statements',
            'peak memory of the whole process while iterating sqlparse.parsestream over 5.5 MB of INSERTs')
    full = 500
    for i, (label, mb, colour) in enumerate(rows):
        y = 86 + i * 38
        s.text(LEFT, y + 14, label, size=14, weight=600 if colour == ACCENT else None)
        s.rect(220, y + 2, full, 16, TRACK, rx=8)
        s.rect(220, y + 2, max(16, full * mb / 410), 16, colour, rx=8)
        s.text(220 + max(16, full * mb / 410) + 10, y + 15, f'{mb} MB', fill=colour if colour != GREY else DIM,
               size=14, weight=700)
    s.text(LEFT, 220, '0.1.5 lexes 1,024 tokens at a time and splits statements off as they are asked for; '
           '0.1.4 lexed the whole text first.', fill=DIM, size=13)
    s.save('memory.svg')


def profile():
    # docs/profile-format.md: sqlparse's format() (reindent + upper-case keywords, and strip_comments) on the corpus.
    parts = [('grouping', 58, BLUE), ('lexing and splitting', 27, PURPLE), ('reindent filter', 9, GREEN),
             ('the rest', 6, GREY)]
    s = Svg(860, 190, "Where sqlparse's format() spends its time",
            'pure-Python sqlparse 0.6.1 formatting the 2.1 MB corpus, by profile (docs/profile-format.md)')
    x, full = LEFT, RIGHT - LEFT
    for i, (name, pct, colour) in enumerate(parts):
        w = full * pct / 100
        s.rect(x + (2 if i else 0), 84, w - 2, 30, colour, rx=6)
        x += w
    for i, (name, pct, colour) in enumerate(parts):
        lx = LEFT + [0, 200, 430, 620][i]
        s.rect(lx, 135, 12, 12, colour, rx=3)
        s.text(lx + 20, 146, f'{pct} %', fill=colour, size=15, weight=700)
        s.text(lx + 64, 146, name, fill=DIM, size=13)
    s.text(LEFT, 176, 'The port runs every grouping pass sqlparse runs, in the same order, and skips only the ones '
           'that cannot find anything.', fill=FAINT, size=12)
    s.save('profile.svg')


def history():
    # docs/internals.md, "The Rust alone": cargo run --release --example bench_format, the corpus, best of 3.
    rows = [('first port', '0.62', '0.50'), ('regex lexer', '0.48', '0.36'), ('fast lexer (0.1.1)', '0.41', '0.30'),
            ('0.1.2', '0.201', '0.126')]
    s = Svg(860, 346, 'The Rust alone, pass by pass',
            'seconds for the 2.1 MB corpus without Python, best of 3, Windows 11')
    s.rect(LEFT, 78, 12, 12, ACCENT, rx=3)
    s.text(LEFT + 20, 89, "format(sql, reindent=True, keyword_case='upper')", fill=DIM, size=13, mono=True)
    s.rect(470, 78, 12, 12, BLUE, rx=3)
    s.text(490, 89, 'parse: lex, split, group', fill=DIM, size=13)
    full = 520
    for i, (name, fmt, parse) in enumerate(rows):
        y = 110 + i * 52
        last = i == len(rows) - 1
        s.text(LEFT, y + 22, name, size=15, weight=700 if last else None, fill=ACCENT if last else TEXT)
        for k, (v, colour) in enumerate([(fmt, ACCENT), (parse, BLUE)]):
            w = full * float(v) / float(rows[0][1])
            s.rect(200, y + 4 + k * 18, w, 13, colour, rx=6)
            s.text(200 + w + 8, y + 15 + k * 18, f'{v} s', fill=DIM, size=12)
    s.text(LEFT, 330,'Each step was checked equal to sqlparse before the next; how: docs/internals.md', fill=FAINT,
           size=12)
    s.save('history.svg')


if __name__ == '__main__':
    os.makedirs(OUT, exist_ok=True)
    how_it_works()
    proof()
    platforms()
    calls()
    memory()
    profile()
    history()
