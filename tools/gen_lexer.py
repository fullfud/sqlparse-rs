"""Generates rust/sqlparse-front/src/lexer_tables.rs from sqlparse itself.

    python -X utf8 tools/gen_lexer.py

Python's `re` and Rust's regex engines disagree on what \\w, \\s, \\d, \\b and case-insensitive letters match
outside ASCII (Python 3.11 uses Unicode 14 and its own \\w; IGNORECASE has its own folding). So no class is
written by hand: every character class, escape and letter in sqlparse's SQL_REGEX is turned into an explicit
set of code points by asking Python's own `re` about every code point, and the patterns are rewritten for
fancy-regex (lookahead and backtracking as in `re`) with those sets. A pattern that opens with a one-character
lookbehind gets it as a separate check on the character before the match, since the Rust side matches on the
text from the match position on. The keyword dictionaries are merged in the lexer's lookup order.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, 'upstream'))

from sqlparse import keywords, tokens  # noqa: E402
from sqlparse.lexer import Lexer  # noqa: E402

FLAGS = re.IGNORECASE | re.UNICODE
OUT = os.path.join(ROOT, 'rust', 'sqlparse-front', 'src', 'lexer_tables.rs')
CODEPOINTS = [chr(c) for c in range(0x110000) if not 0xD800 <= c <= 0xDFFF]

_sets = {}


def charset(item):
    """The code points `item` (a regex matching one character) matches under the lexer's flags."""
    if item not in _sets:
        m = re.compile(item, FLAGS).fullmatch
        _sets[item] = [c for c in CODEPOINTS if m(c)]
    return _sets[item]


def ranges(chars):
    out = []
    for c in chars:
        c = ord(c)
        if out and out[-1][1] == c - 1:
            out[-1][1] = c
        else:
            out.append([c, c])
    return out


def rust_class(chars, negate=False):
    parts = []
    for a, b in ranges(chars):
        parts.append(f'\\x{{{a:X}}}' if a == b else f'\\x{{{a:X}}}-\\x{{{b:X}}}')
    return '[' + ('^' if negate else '') + ''.join(parts) + ']'


def atom(item):
    chars = charset(item)
    if len(chars) == 1:
        return f'\\x{{{ord(chars[0]):X}}}'
    return rust_class(chars)


WORD = r'\w'

_atoms = {}


def atom_set(op, av):
    """Code points one parsed single-character node (literal, class, category, any) matches, from `re` itself."""
    from re import _compiler, _parser
    key = repr((op, av))
    if key not in _atoms:
        sp = _parser.parse('x', FLAGS)
        sp.data = [(op, av)]
        m = _compiler.compile(sp, FLAGS).fullmatch
        _atoms[key] = {c for c in CODEPOINTS if m(c)}
    return _atoms[key]


def first_chars(rx):
    """Every code point a match of rx can start with (an over-approximation is safe: it only skips rules
    that cannot match). Zero-width assertions and anchors add nothing and let the next item through."""
    from re import _constants as C, _parser
    single = {C.LITERAL, C.NOT_LITERAL, C.IN, C.ANY}

    def seq(items):
        acc = set()
        for op, av in items:
            s, nullable = node(op, av)
            acc |= s
            if not nullable:
                return acc, False
        return acc, True

    def node(op, av):
        if op in single:
            return atom_set(op, av), False
        if op is C.SUBPATTERN:
            return seq(av[-1])
        if op is C.BRANCH:
            acc, nullable = set(), False
            for alt in av[1]:
                s, n = seq(alt)
                acc |= s
                nullable |= n
            return acc, nullable
        if op in (C.MAX_REPEAT, C.MIN_REPEAT, getattr(C, 'POSSESSIVE_REPEAT', None)):
            lo, _hi, item = av
            s, n = seq(item)
            return s, lo == 0 or n
        if op in (C.ASSERT, C.ASSERT_NOT, C.AT):
            return set(), True
        raise ValueError(f'first_chars: unhandled {op} in {rx}')

    s, nullable = seq(_parser.parse(rx, FLAGS))
    if nullable:
        raise ValueError(f'rule can match empty: {rx}')
    return sorted(s)


def translate(pattern):
    """(rust pattern, before) for one SQL_REGEX pattern; before is None or ('in'|'not', item)."""
    before = None
    m = re.match(r'\(\?(<=|<!)(\\.|\[(?:\\.|[^\]])*\])\)', pattern)
    if m:
        before = ('in' if m.group(1) == '<=' else 'not', m.group(2))
        pattern = pattern[m.end():]
    out = []
    i = 0
    while i < len(pattern):
        c = pattern[i]
        if c == '\\':
            e = pattern[i + 1]
            if e == 'b':
                # every \b in SQL_REGEX follows a word character: a boundary is "no word character next"
                out.append('(?!' + rust_class(charset(WORD)) + ')')
            else:
                out.append(atom(pattern[i:i + 2]))
            i += 2
        elif c == '[':
            j = i + 1
            if pattern[j] == '^':
                j += 1
            while pattern[j] != ']':
                j += 2 if pattern[j] == '\\' else 1
            out.append(rust_class(charset(pattern[i:j + 1])))
            i = j + 1
        elif c == '(':
            if pattern.startswith('(?:', i) or pattern.startswith('(?=', i) or pattern.startswith('(?!', i):
                out.append(pattern[i:i + 3])
                i += 3
            elif pattern.startswith('(?<', i):
                raise ValueError(f'lookbehind inside a pattern: {pattern}')
            else:
                out.append('(?:')  # captures are never read: keep the groups, drop the numbering
                i += 1
        elif c in ')|*+?.$':
            out.append(c)
            i += 1
        elif c == '{':
            j = pattern.index('}', i)
            out.append(pattern[i:j + 1])
            i = j + 1
        else:
            out.append(atom(re.escape(c)))
            i += 1
    return ''.join(out), before


def ttype_name(t):
    return str(t)


def main():
    lexer = Lexer.get_default_instance()
    rules = []
    for rx, action in keywords.SQL_REGEX:
        pat, before = translate(rx)
        if action is keywords.PROCESS_AS_KEYWORD:
            act = 'Action::Keyword'
        else:
            act = f'Action::Token("{ttype_name(action)}")'
        if before is None:
            b = 'Before::Any'
        else:
            kind, item = before
            b = f'Before::{"In" if kind == "in" else "NotIn"}(&{rust_ranges_const(charset(item))})'
        first = rust_ranges_const(first_chars(rx))
        rules.append(f'    // {rx}\n    Rule {{ pattern: r"{pat}", before: {b}, first: &{first}, action: {act} }},')

    merged = {}
    for kwdict in lexer._keywords:
        for k, v in kwdict.items():
            merged.setdefault(k, ttype_name(v))
    kw = ',\n'.join(f'    ("{k}", "{v}")' for k, v in sorted(merged.items(), key=lambda kv: kv[0].encode('utf-8')))

    dollar, _ = translate(r'\$(?:[_A-ZÀ-Ü]\w*)?\$')
    opener_not = rust_ranges_const(charset(r'[\w"$]'))

    src = f'''//! Generated by tools/gen_lexer.py from sqlparse {sys.modules["sqlparse"].__version__}: do not edit.
//! Character sets are Python 3.11 `re` (Unicode 14) under IGNORECASE | UNICODE, asked code point by code point.

#![allow(clippy::all)]

pub(crate) enum Before {{
    Any,
    In(&'static [(u32, u32)]),
    NotIn(&'static [(u32, u32)]),
}}

pub(crate) enum Action {{
    Token(&'static str),
    Keyword,
}}

pub(crate) struct Rule {{
    pub pattern: &'static str,
    pub before: Before,
    /// Every character a match can start with (from Python's own parse of the pattern).
    pub first: &'static [(u32, u32)],
    pub action: Action,
}}

pub(crate) static RULES: &[Rule] = &[
{chr(10).join(rules)}
];

/// Every keyword of every dictionary the default lexer loads, first dictionary wins, sorted by bytes.
pub(crate) static KEYWORDS: &[(&str, &str)] = &[
{kw}
];

/// `\\$(?:[_A-ZÀ-Ü]\\w*)?\\$`: a dollar-quote delimiter.
pub(crate) static DOLLAR_DELIM: &str = r"{dollar}";

/// `[\\w"$]`: a dollar-quote delimiter after one of these does not open a literal.
pub(crate) static DOLLAR_OPENER_NOT_AFTER: &[(u32, u32)] = &{opener_not};
'''
    with open(OUT, 'w', encoding='utf-8', newline='\n') as f:
        f.write(src)
    print(f'{len(rules)} rules, {len(merged)} keywords, {len(_sets)} character sets -> {OUT}')


def rust_ranges_const(chars):
    return '[' + ', '.join(f'(0x{a:X}, 0x{b:X})' for a, b in ranges(chars)) + ']'


if __name__ == '__main__':
    main()
