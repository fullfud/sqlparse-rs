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
   as plain typed functions over strings, plus `parse_tree`, `parse()`'s statement tree as plain lists, so the
   tree itself is checked node by node. Only strings and plain data cross the bridge.
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
| 2 | `split` | `engine/statement_splitter.py` | **done**: 1123/1123 recorded, 0 of 10 million checked texts differ, 12x faster |
| 3 | `format`, `parse_tree` | `engine/grouping.py`, `sql.py` (the token tree), `filters/*`, `formatter.py` | **done**: every recorded tree and format result matches (see below), 17-22x faster |
| 4 | package | a wheel whose `format`/`split` use Rust and fall back to sqlparse for anything not ported | |

Where the time went in sqlparse (`docs/profile-format.md`): grouping 58 %, lexing and splitting 27 %, the
reindent filters 9 %. `parse()` returns sqlparse's Python token tree to callers, so it stays in Python;
`format()` and `split()` are the calls worth replacing whole, and both run entirely in Rust now.

## Numbers

Baseline, sqlparse 0.6.1 on CPython 3.11, the 2.1 MB benchmark corpus (`bench/workload.py`):

| Call | Python |
| --- | ---: |
| `split` | 3.5 s |
| `format` (reindent + upper keywords, and strip comments) | 23.0 s |
| `parse` | 10.3 s |

Recording: 7842 cases (format 5427, split 1123, tokenize 666, parse_tree 626; 327 of them errors the
original raises). `rust-anything diff`: **7838/7842 match**; the other 4 raise the same message under another
exception class (see Known differences).

All of it, same corpus, through the bridge (best of 1; `cargo run --release --example bench_format` for Rust
alone):

| Call | Python | Rust | Rust alone |
| --- | ---: | ---: | ---: |
| `split` | 4.14 s | **0.35 s (12x)** | |
| `format(keyword_case='upper')` (no tree) | 4.01 s | **0.30 s (13x)** | |
| `format(reindent=True, keyword_case='upper')` | 15.42 s | **0.90 s (17x)** | 0.66 s |
| `format(strip_comments=True)` | 13.63 s | **0.82 s (17x)** | 0.53 s |
| `format(reindent_aligned=True)` | 13.91 s | **0.62 s (22x)** | 0.62 s |
| `parse` (lex, split, group: the tree) | 12.74 s | | 0.51 s |

Lexer (step 1), same corpus, best of 3:

| | Python | Rust |
| --- | ---: | ---: |
| `tokenize`, Rust alone (`cargo run --release --example bench_lexer`) | 2.86 s | **0.19 s (15x)** |
| `tokenize` through the bridge (800k tuples back to Python) | 2.86 s | 0.95 s (3x) |

`format()` and `split()` return one string, so they do not pay the bridge's per-token cost.

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

### How the tree matches sqlparse's

`src/tree.rs` is `sql.py` on an arena, `src/grouping.rs` is `grouping.py` function by function in `group()`'s
order, `src/filters.rs` the filters that work on the tree. What sqlparse does by accident is kept, because
output depends on it:

- a group's `value` is joined when the group is made and never refreshed: the reindent filter measures
  identifiers by it, and the Python/PHP output filters print it, after strip_whitespace has changed the
  leaves (`str()` is recomputed from the leaves, as in Python);
- `normalized`, `is_keyword` and `is_whitespace` are taken at creation (`group_operator` later retypes `*`);
- `_group` walks a copy of the list taken at the start, so a token already moved into a group is still looked
  at, with an index that is off; `tidx_offset` and the stale `prev_` are reproduced;
- `ttype in T.X` (subtypes) against `ttype in (T.X, T.Y)` (equality) and `isinstance(x, TokenList)` (any group,
  which `align_comments` extends) are told apart everywhere;
- tokens put in with `tokens.insert` have no parent, which `_current_line_len` and `within()` see;
- with an output format, the statement's tokens become a generator the serializer uses up, so the reindent
  filter separates the next statement as if the last one were `''`;
- the limits raise what sqlparse raises: grouping depth 100, 10,000 tokens, `right_margin` is
  `NotImplementedError`, and Python's own recursion limit, which ~980 nested BEGIN blocks or BETWEEN chains
  reach: the port counts the frames sqlparse would use from the caller's real stack depth (asking Python for
  the exact headroom near the limit), so it fails from exactly the same size (`tools/check_recursion.py`).

Checks besides the recording: `tools/check_tree.py` (every corpus and sqlparse test file, cuts of them,
40,000 random scripts and limit inputs: 260,837 calls of `parse_tree` and 18 format option sets, 0 differ),
`tools/check_format_flat.py` (26.7 million case-filter texts over every code point, 0 differ),
`rust-anything fuzz` (161,856 fresh cases: only the known differences).

### Known differences

- **Exception class.** sqlparse raises `sqlparse.exceptions.SQLParseError`; the generated bridge can only raise
  built-in exceptions, so the port raises `ValueError('SQLParseError: <same message>')`. rust-anything compares
  classes, so these cases count as mismatches. The package (step 4) re-raises sqlparse's class.
- **`None` for a bool option.** sqlparse treats `format(sql, reindent=None)` as False; the bindings take a
  bool and raise TypeError. Outside the declared types; the package passes such calls to sqlparse.
- The recursion limit is modelled for CPython 3.11, where it was measured.

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
