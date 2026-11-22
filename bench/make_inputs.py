"""Writes the real cases rust-anything records on top of its generated ones: rust/sqlparse-front/inputs.jsonl.

    python bench/make_inputs.py

Inputs: every corpus file (bench/corpus, run gen_corpus.py first) and every SQL file of sqlparse's own test
suite. Each goes through tokenize, split (both ways) and format with the option sets people use, and parse_tree;
the large scripts get only the common format calls and no tree, to keep the recording a sensible size.
"""

import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

FORMATS = [
    {},
    {'reindent': True, 'keyword_case': 'upper'},
    {'strip_comments': True},
    {'reindent_aligned': True},
    {'keyword_case': 'lower', 'identifier_case': 'upper'},
    {'use_space_around_operators': True, 'strip_whitespace': True},
    {'reindent': True, 'comma_first': True},
    {'reindent': True, 'wrap_after': 40},
    {'reindent': True, 'indent_tabs': True, 'indent_after_first': True},
    {'truncate_strings': 10},
    {'output_format': 'python'},
    {'reindent': True, 'compact': True, 'indent_width': 4},
]
COMMON = FORMATS[:3]


def sources():
    corpus = os.path.join(HERE, 'corpus')
    for name in sorted(os.listdir(corpus)):
        yield name, read(os.path.join(corpus, name))
    files = os.path.join(ROOT, 'upstream', 'tests', 'files')
    for name in sorted(os.listdir(files)):
        text = read(os.path.join(files, name))
        if text is not None:
            yield name, text


def read(path):
    try:
        with open(path, encoding='utf-8') as f:
            return f.read()
    except UnicodeDecodeError:
        return None  # the cp1251/gbk files test sqlparse's own decoding, not a str front door


def main():
    out = os.path.join(ROOT, 'rust', 'sqlparse-front', 'inputs.jsonl')
    n = 0
    with open(out, 'w', encoding='utf-8', newline='\n') as f:
        def case(fn, *args, **kwargs):
            nonlocal n
            row = {'fn': fn, 'args': list(args)}
            if kwargs:
                row['kwargs'] = kwargs
            f.write(json.dumps(row, ensure_ascii=False) + '\n')
            n += 1
        for name, sql in sources():
            big = name.startswith('script') or len(sql) > 20000
            case('tokenize', sql)
            case('split', sql)
            case('split', sql, strip_semicolon=True)
            for opts in (COMMON if big else FORMATS):
                case('format', sql, **opts)
            if not big:
                case('parse_tree', sql)
    print(f'{n} cases to {out}')


if __name__ == '__main__':
    main()
