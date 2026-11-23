"""Draws the README's race animation (docs/assets/race.gif) and speed chart (docs/assets/speed.svg) from the
times bench/race_times.py measured. The race runs in real time: each side moves on to the next file when its
measured time for the file has passed.

    python bench/race_times.py race.json && python tools/make_visuals.py race.json
"""

import json
import os
import sys

from PIL import Image, ImageDraw, ImageFont
from sqlparse import tokens as T
from sqlparse.lexer import tokenize

import sqlparse_turbo

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, 'docs', 'assets')
FONTS = 'C:/Windows/Fonts/'

W, H = 1100, 560
FPS = 25
BG = (13, 14, 18)
CARD = (22, 23, 29)
BORDER = (42, 44, 52)
BORDER_HOT = (120, 72, 40)
TEXT = (236, 237, 240)
DIM = (150, 153, 162)
FAINT = (95, 98, 108)
TRACK = (34, 36, 44)
GREY_BAR = (140, 143, 152)
ACCENT = (255, 138, 61)
GREEN = (126, 211, 133)
PURPLE = (190, 150, 255)
BLUE = (110, 170, 255)


def font(name, size):
    return ImageFont.truetype(FONTS + name, size)


F_TITLE = font('segoeuib.ttf', 24)
F_SUB = font('segoeui.ttf', 17)
F_CARD = font('segoeuib.ttf', 22)
F_SMALL = font('segoeui.ttf', 15)
F_SMALL_B = font('segoeuib.ttf', 15)
F_BIG = font('CascadiaMono.ttf', 58)
F_MONO = font('CascadiaMono.ttf', 15)
F_CODE = font('CascadiaMono.ttf', 13)


def colored_lines(sql, limit):
    """The text's first lines as [(text, colour)] runs, coloured by sqlparse's own token types."""
    lines, cur = [], []
    for ttype, value in tokenize(sql):
        if ttype in T.Keyword or ttype in T.Name.Builtin:
            colour = ACCENT
        elif ttype in T.String:
            colour = GREEN
        elif ttype in T.Number:
            colour = PURPLE
        elif ttype in T.Comment:
            colour = FAINT
        elif ttype in T.Operator or ttype in T.Punctuation:
            colour = DIM
        else:
            colour = TEXT
        parts = value.split('\n')
        for i, part in enumerate(parts):
            if i:
                lines.append(cur)
                cur = []
                if len(lines) >= limit:
                    return lines
            if part:
                cur.append((part, colour))
    lines.append(cur)
    return lines[:limit]


def rounded(d, box, fill, outline, width=1, r=14):
    d.rounded_rectangle(box, r, fill=fill, outline=outline, width=width)


def bar(d, x, y, w, frac, colour):
    d.rounded_rectangle((x, y, x + w, y + 14), 7, fill=TRACK)
    if frac > 0:
        d.rounded_rectangle((x, y, x + max(14, int(w * frac)), y + 14), 7, fill=colour)


def card(d, x, y, w, h, name, tag, cmd, t, done_n, total, frac, colour, code, finished, digest):
    rounded(d, (x, y, x + w, y + h), CARD, BORDER_HOT if finished and colour == ACCENT else BORDER)
    d.text((x + 24, y + 20), name, font=F_CARD, fill=TEXT)
    d.text((x + w - 24, y + 26), tag, font=F_SMALL, fill=DIM, anchor='ra')
    d.text((x + 24, y + 56), '$', font=F_MONO, fill=DIM)
    d.text((x + 40, y + 56), cmd, font=F_MONO, fill=TEXT)
    d.text((x + 22, y + 84), f'{t:.2f} s', font=F_BIG, fill=colour if colour == ACCENT else TEXT)
    d.text((x + 24, y + 158), f'{done_n} / {total} files formatted', font=F_SMALL, fill=DIM)
    bar(d, x + 24, y + 186, w - 48, frac, colour)
    # the output of the file being formatted
    cy = y + 218
    rounded(d, (x + 24, cy, x + w - 24, cy + 150), (16, 17, 22), (32, 34, 41), r=8)
    for i, runs in enumerate(code):
        cx = x + 38
        for text, c in runs:
            if cx > x + w - 60:
                break
            d.text((cx, cy + 12 + i * 19), text, font=F_CODE, fill=c)
            cx += d.textlength(text, font=F_CODE)
    if finished:
        tick = GREEN if colour == ACCENT else TEXT
        d.line([(x + 25, y + h - 25), (x + 30, y + h - 20), (x + 39, y + h - 31)], fill=tick, width=3, joint='curve')
        d.text((x + 46, y + h - 34), 'done', font=F_SMALL_B, fill=tick)
        d.text((x + w - 24, y + h - 34), f'sha256 {digest[:16]}…', font=F_SMALL, fill=DIM, anchor='ra')


def race(data, outputs):
    call = data['calls']['format_reindent']
    a, b = call['sqlparse'], call['turbo']
    n = len(a)
    cum_a = [sum(a[:i + 1]) for i in range(n)]
    cum_b = [sum(b[:i + 1]) for i in range(n)]
    total_a, total_b = cum_a[-1], cum_b[-1]
    code_cache = {}

    def code_for(i):
        if i not in code_cache:
            code_cache[i] = colored_lines(outputs[min(i, n - 1)], 7)
        return code_cache[i]

    def done_at(cum, t):
        k = 0
        while k < n and cum[k] <= t:
            k += 1
        return k

    intro, hold = 0.6, 3.2
    frames, durations = [], []
    steps = int((total_a + intro + hold) * FPS) + 1
    for f in range(steps):
        t = f / FPS - intro
        d_img = Image.new('RGB', (W, H), BG)
        d = ImageDraw.Draw(d_img)
        d.text((W // 2, 34), 'sqlparse.format(sql, reindent=True, keyword_case=\'upper\')', font=F_TITLE,
               fill=TEXT, anchor='mm')
        d.text((W // 2, 66), f"{n} SQL queries ({data['bytes'] // 1024} KB) · Python {data['python']} · "
               'one call per file, each at its measured time', font=F_SUB, fill=DIM, anchor='mm')
        tt = max(t, 0)
        ka, kb = done_at(cum_a, tt), done_at(cum_b, tt)
        fin_a, fin_b = ka >= n, kb >= n
        cw, top, ch = 515, 96, 410
        card(d, 28, top, cw, ch, 'sqlparse', f"pure Python, {data['sqlparse']}", 'sqlparse.format(sql, …)',
             min(tt, total_a), ka, n, ka / n, GREY_BAR, code_for(ka), fin_a, call['sha256'])
        card(d, W - 28 - cw, top, cw, ch, 'sqlparse-turbo', 'Rust, same API', 'sqlparse_turbo.format(sql, …)',
             min(tt, total_b), kb, n, kb / n, ACCENT, code_for(kb), fin_b, call['sha256'])
        if fin_a:
            msg = [('Same output, byte for byte (same sha256). ', TEXT),
                   (f'{total_a / total_b:.0f}x faster', ACCENT)]
        elif fin_b:
            msg = [('sqlparse-turbo is done. sqlparse is still going…', DIM)]
        else:
            msg = [('Both start on the same 400 files…', DIM)]
        width = sum(d.textlength(s, font=F_SUB) for s, _ in msg)
        x = (W - width) / 2
        for s, c in msg:
            d.text((x, H - 34), s, font=F_SUB, fill=c, anchor='lm')
            x += d.textlength(s, font=F_SUB)
        if frames and f < steps - 1 and d_img.tobytes() == frames[-1].tobytes():
            durations[-1] += 1000 // FPS
            continue
        frames.append(d_img)
        durations.append(1000 // FPS)
    durations[-1] += 600
    path = os.path.join(OUT, 'race.gif')
    pal = frames[-1].quantize(colors=128, method=Image.Quantize.MEDIANCUT)
    q = [fr.quantize(palette=pal, dither=Image.Dither.NONE) for fr in frames]
    q[0].save(path, save_all=True, append_images=q[1:], duration=durations, loop=0, optimize=True, disposal=1)
    print(path, len(frames), 'frames', round(os.path.getsize(path) / 1e6, 2), 'MB')
    frames[-1].save(os.path.join(OUT, 'race-last.png'))


def chart(data):
    rows = [
        ('split(sql)', 'split'),
        ("format(sql, keyword_case='upper')", 'format_upper'),
        ("format(sql, reindent=True, keyword_case='upper')", 'format_reindent'),
        ('parse(sql)', 'parse'),
    ]
    w, row_h, top = 860, 74, 78
    h = top + row_h * len(rows) + 46
    left, right = 28, w - 28
    full = right - left - 130  # room for the label after the longest bar
    longest = max(sum(data['calls'][k]['sqlparse']) for _, k in rows)
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" '
           'font-family="Segoe UI, -apple-system, Helvetica, Arial, sans-serif">',
           f'<rect width="{w}" height="{h}" rx="14" fill="#0d0e12"/>',
           f'<text x="{left}" y="38" fill="#ecedf0" font-size="20" font-weight="700">sqlparse vs sqlparse-turbo, '
           f'{data["files"]} SQL queries</text>',
           f'<text x="{left}" y="60" fill="#9699a2" font-size="14">seconds for all files, one call per file, '
           f'Python {data["python"]}; every result equal to sqlparse\'s</text>']
    for i, (label, key) in enumerate(rows):
        c = data['calls'][key]
        a, b = sum(c['sqlparse']), sum(c['turbo'])
        y = top + i * row_h
        out.append(f'<text x="{left}" y="{y + 18}" fill="#ecedf0" font-size="14" '
                   f'font-family="Cascadia Mono, Consolas, Menlo, monospace">{label}</text>')
        out.append(f'<text x="{right}" y="{y + 18}" fill="#ff8a3d" font-size="15" font-weight="700" '
                   f'text-anchor="end">{a / b:.0f}x faster</text>')
        wa = full * a / longest
        wb = max(8, full * b / longest)
        out.append(f'<rect x="{left}" y="{y + 28}" width="{wa:.1f}" height="14" rx="7" fill="#8c8f98"/>')
        out.append(f'<text x="{left + wa + 8:.1f}" y="{y + 40}" fill="#9699a2" font-size="13">sqlparse '
                   f'{a:.2f} s</text>')
        out.append(f'<rect x="{left}" y="{y + 48}" width="{wb:.1f}" height="14" rx="7" fill="#ff8a3d"/>')
        out.append(f'<text x="{left + wb + 8:.1f}" y="{y + 60}" fill="#ff8a3d" font-size="13">sqlparse-turbo '
                   f'{b:.3f} s</text>')
    out.append(f'<text x="{left}" y="{h - 18}" fill="#5f626c" font-size="12">bench/race_times.py: best of 3 per '
               f'file, {data["bytes"] // 1024} KB of SQL</text>')
    out.append('</svg>')
    path = os.path.join(OUT, 'speed.svg')
    with open(path, 'w', encoding='utf-8', newline='\n') as f:
        f.write('\n'.join(out) + '\n')
    print(path)


def main():
    with open(sys.argv[1], encoding='utf-8') as f:
        data = json.load(f)
    os.makedirs(OUT, exist_ok=True)
    import glob
    paths = sorted(glob.glob(os.path.join(ROOT, 'bench', 'corpus', 'q*.sql')))
    outputs = [sqlparse_turbo.format(open(p, encoding='utf-8').read(), reindent=True, keyword_case='upper')
               for p in paths]
    chart(data)
    race(data, outputs)


main()
