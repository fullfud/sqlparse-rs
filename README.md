# sqlparse-rs

[sqlparse](https://github.com/andialbrecht/sqlparse) rewritten in Rust, with proof that it gives the same
results as the original: the same output for every recorded input, down to the byte.

sqlparse is a pure-Python SQL parser and formatter. It gets about 30 million downloads a week on PyPI, Django
depends on it, and people complain that it is slow, sometimes quadratically so. No Rust library gives the
same output: sqloxide and sqlformat-rs parse SQL differently. This project closes that gap. It is a drop-in
`format()` and `split()` that runs much faster and returns what sqlparse returns.

## How it works

The Rust is checked with [rust-anything](https://github.com/erkamyaman/rust-anything):

1. `upstream/` is the original, a git submodule pinned to release **0.6.1**. Nothing in it is changed.
2. `front/sqlparse_front.py` is the front door: `tokenize`, `split` and `format` (with all of its options)
   as plain typed functions over strings. Only strings cross the bridge; the token tree stays on one side.
3. `rust/sqlparse-front/` is the Rust crate (PyO3). The port is written in `src/port.rs` and the files it uses.
4. `rust-anything record` runs the original on generated edge cases plus real SQL (`bench/make_inputs.py`:
   the benchmark corpus and sqlparse's own test files, through every format option set people use) and
   saves what each call returns or raises. `diff` replays all of it on the Rust; a piece is done when every
   case matches and `gate` passes.

## Plan

Port in the order data flows through sqlparse, each step proven before the next:

| Step | Front-door function | sqlparse code | State |
| --- | --- | --- | --- |
| 1 | `tokenize` | `tokens.py`, `keywords.py`, `lexer.py` (the regex lexer and keyword tables) | **done**: 666/666 recorded, 0 of 32,986 checked texts differ, 15x faster |
| 2 | `split` | `engine/statement_splitter.py` | |
| 3 | `format` | `engine/grouping.py`, `sql.py` (the token tree), `filters/*`, `formatter.py` | |
| 4 | package | a wheel whose `format`/`split` use Rust and fall back to sqlparse for anything not ported | |

Where the time goes today (`docs/profile-format.md`): grouping 58 %, lexing and splitting 27 %, the
reindent filters 9 %. `parse()` returns sqlparse's Python token tree to callers, so it stays in Python;
`format()` and `split()` are the calls worth replacing whole.

## Numbers

Baseline, sqlparse 0.6.1 on CPython 3.11, the 2.1 MB benchmark corpus (`bench/workload.py`):

| Call | Python |
| --- | ---: |
| `split` | 3.5 s |
| `format` (reindent + upper keywords, and strip comments) | 23.0 s |
| `parse` | 10.3 s |

Recording: 7216 cases (format 5427, split 1123, tokenize 666; 327 of them errors the original raises).

Lexer (step 1), same corpus, best of 3:

| | Python | Rust |
| --- | ---: | ---: |
| `tokenize`, Rust alone (`cargo run --release --example bench_lexer`) | 2.86 s | **0.19 s (15x)** |
| `tokenize` through the bridge (800k tuples back to Python) | 2.86 s | 0.95 s (3x) |

`format()` and `split()` return one string, so they will not pay the bridge's per-token cost.

### How the lexer matches Python's `re`

Python 3.11 and Rust disagree on `\w`, `\s`, `\d`, `` and case-insensitive letters outside ASCII (Unicode 14
against a newer table, combining marks, numerics like `½`, `ſ` and the Kelvin sign). `tools/gen_lexer.py`
writes `src/lexer_tables.rs` from sqlparse itself: every character class in `SQL_REGEX` becomes the exact set
of code points Python's `re` matches, asked code point by code point; patterns run on fancy-regex so
lookahead and backtracking behave as in `re` (`123abc` lexes as `12` + `3abc` in both); one-character
lookbehinds are checked on the character before the match, which `re.match(text, pos)` also sees. Each rule
also gets the set of characters a match can start with, from Python's own parse of the pattern, so a
position only tries the rules that can match there.

`tools/check_tokenize.py` compares both on every recorded text, three random cuts of each (unclosed strings,
comments, dollar quotes) and 30,000 random texts built from the pieces where the engines part.

## Run it

```sh
git submodule update --init
python -m venv .venv && .venv/Scripts/python -m pip install -e ./upstream pytest
.venv/Scripts/python -X utf8 bench/gen_corpus.py          # the corpus (not committed, same seed = same bytes)
.venv/Scripts/python -X utf8 bench/workload.py            # Python timings
.venv/Scripts/python -X utf8 bench/make_inputs.py         # real cases for the recording
# with .venv/Scripts first on PATH, so rust-anything's Python sees the pinned sqlparse:
rust-anything record rust/sqlparse-front --inputs rust/sqlparse-front/inputs.jsonl
rust-anything build rust/sqlparse-front
rust-anything diff rust/sqlparse-front
rust-anything gate rust/sqlparse-front
```

## License

BSD-3-Clause, like sqlparse (`LICENSE`). The port follows sqlparse's code closely, so sqlparse's copyright
notice is kept next to ours.
