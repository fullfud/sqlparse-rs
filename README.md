<h1 align="center">sqlparse-turbo</h1>

<p align="center">
  <a href="https://pypi.org/project/sqlparse-turbo/"><img src="https://img.shields.io/pypi/v/sqlparse-turbo?color=FF8A3D&label=PyPI" alt="PyPI version"></a>
  <a href="https://pypi.org/project/sqlparse-turbo/"><img src="https://img.shields.io/pypi/pyversions/sqlparse-turbo" alt="Python versions"></a>
  <a href="https://github.com/fullfud/sqlparse-turbo/actions/workflows/ci.yml"><img src="https://github.com/fullfud/sqlparse-turbo/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/Linux%20·%20macOS%20·%20Windows-wheels-2563EB" alt="Wheels for Linux, macOS and Windows">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-BSD--3--Clause-555" alt="BSD-3-Clause license"></a>
</p>

<p align="center"><b><a href="https://github.com/andialbrecht/sqlparse">sqlparse</a>'s <code>format()</code>,
<code>split()</code> and <code>parse()</code>, rewritten in Rust:<br>same output, byte for byte, 22-145 times
faster.</b></p>

<p align="center">
  <img src="https://raw.githubusercontent.com/fullfud/sqlparse-turbo/main/docs/assets/race.gif" alt="sqlparse.format(reindent=True, keyword_case='upper') over 400 SQL queries, each file at its measured time: sqlparse takes 3.73 s, sqlparse-turbo 0.058 s (64x), and both give the same output (same sha256)." width="820">
</p>

```sh
pip install sqlparse-turbo "sqlparse==0.6.1"
```

> [!IMPORTANT]
> **The speed-up needs sqlparse 0.6.1**, sqlparse's current release. The Rust is a port of that version and runs
> only with it. With any other sqlparse the package still installs and every answer is still right, but every
> call simply goes to sqlparse at sqlparse's own speed: `install()` returns False, `sqlparse_turbo.ACTIVE` is
> False, and importing the package warns about it.

sqlparse is the pure-Python SQL splitter and formatter that Django depends on (every SQL script a migration
runs goes through `sqlparse.split` and `sqlparse.format`, and the test runner formats debug SQL with it), along
with many other tools and scripts. It is also slow: formatting two megabytes of SQL takes over ten seconds. Rust SQL libraries
(sqlparser-rs, sqlformat-rs, sqloxide) are fast, but they format differently, so switching to them changes
your output. This project ports sqlparse itself, quirks included, and proves the port gives the same answer
as the original on every input it has been shown.

```python
import sqlparse_turbo

sqlparse_turbo.format(sql, reindent=True, keyword_case='upper')   # == sqlparse.format(sql, reindent=True, ...)
sqlparse_turbo.split(sql)                                          # == sqlparse.split(sql)
sqlparse_turbo.split_many(sqls)                                    # split() of each, on all CPU threads
sqlparse_turbo.parse(sql)                                          # == sqlparse.parse(sql): sqlparse's own objects

sqlparse_turbo.install()    # from now on sqlparse.format / sqlparse.split are the Rust ones, for every caller,
                            # and sqlparse.parse and sqlparse's lexer too
```

```sh
sqlformat-turbo -r -k upper query.sql      # sqlparse's own `sqlformat` command, formatting in Rust
```

## Django

Django and its tools call sqlparse themselves. Add the app to `settings.py` and all of them get the Rust, with
nothing else changed:

```python
INSTALLED_APPS = [
    ...,
    "sqlparse_turbo.django",
]
```

When Django starts, the app calls `sqlparse_turbo.install()`. With a sqlparse other than 0.6.1 it replaces
nothing, and `manage.py check` says so (`sqlparse_turbo.W001`), so a project that pins another sqlparse does not
quietly run at sqlparse speed. Calling `sqlparse_turbo.install()` yourself, at the top of `settings.py`, does the
same without the app.

- **[django-debug-toolbar](https://github.com/django-commons/django-debug-toolbar)** formats every query of a
  page for its SQL panel with its own sqlparse filter stack. With `install()` the statements come grouped from
  the Rust and only the toolbar's own filters run in Python: the panel formats the 400 corpus queries
  **4.3x faster** (4.80 s → 1.12 s), the HTML identical, and a page making 200 ORM queries loads **2.15x faster**
  through the toolbar (0.56 s → 0.26 s, the rest being the queries themselves and the templates).
  Measured with [`bench/django_toolbar.py`](bench/django_toolbar.py), Django 6.1, django-debug-toolbar 8.0,
  Python 3.13.
- **Migrations** with `RunSQL` scripts and `sqlmigrate` go through `sqlparse.split` and `sqlparse.format`.
- **Tests**: the runner formats debug SQL with `sqlparse.format`.

## Results

<p align="center">
  <img src="https://raw.githubusercontent.com/fullfud/sqlparse-turbo/main/docs/assets/speed.svg" alt="Seconds for 400 SQL queries, one call per file, Python 3.13: split 0.96 s against 0.0066 s (145x), format with keyword_case 1.00 s against 0.0095 s (106x), format with reindent 3.73 s against 0.058 s (64x), parse 2.95 s against 0.131 s (22x)." width="760">
</p>

The chart and the animation above come from [`bench/race_times.py`](bench/race_times.py) (best of 3 per file,
every result checked equal) and [`tools/make_visuals.py`](tools/make_visuals.py).

The whole benchmark corpus (440 files, 2.1 MB of SQL: generated queries and scripts), sqlparse 0.6.1 against
`sqlparse_turbo` through the same public calls, CPython 3.11, Windows 11, best of 3, every result checked equal
([`bench/bench_package.py`](bench/bench_package.py)):

| Call | sqlparse | sqlparse_turbo | Speed-up |
| --- | ---: | ---: | ---: |
| `split(sql)` | 4.30 s | 0.037 s | **116x** |
| `format(sql, keyword_case='upper')` | 4.82 s | 0.046 s | **105x** |
| `format(sql, reindent=True, keyword_case='upper')` | 13.57 s | 0.218 s | **62x** |
| `format(sql, strip_comments=True)` | 11.69 s | 0.152 s | **77x** |
| `format(sql, reindent_aligned=True)` | 12.33 s | 0.179 s | **69x** |
| `format(sql, reindent=True, indent_columns=True, comma_first=True)` | 13.18 s | 0.197 s | **67x** |

`split_many` over the 400 query files takes 0.003 s against sqlparse's 1.07 s (12 threads), and with
`install()` sqlparse's `lexer.tokenize` is 20x faster.

`parse(sql)` over the 400 query files: sqlparse 3.0 s, sqlparse_turbo 0.13-0.18 s (**17-22x**). The statements are grouped in Rust and handed
back as sqlparse's own `Statement`, `Identifier`, ... objects, made with `object.__new__` and their slots set
from the Rust tree, so a group's value joined when the group was made, `normalized` taken at creation and
every `parent` pointer are sqlparse's. A tree is a reference cycle (`parent` and `tokens`), so freeing one
waits for the garbage collector, in sqlparse as here: that collection is most of what is left.

### Against markocerkez/sqlparse-rs

[markocerkez/sqlparse-rs](https://github.com/markocerkez/sqlparse-rs) (PyPI `sqlparse-rs` 0.1.0, imported as
`sqlparse_rs`) ports sqlparse's lexer and `split` to Rust and leaves grouping and the filters, so `format`, in
Python. Both packages on the same 400 query files
(579 KB), same machine, best of 3, every result equal to sqlparse's:

| Call | sqlparse | markocerkez | this one |
| --- | ---: | ---: | ---: |
| `split`, each file (CPython 3.13) | 1.00 s | 0.014 s | **0.008 s** |
| `format(keyword_case='upper')` (3.13) | 1.04 s | 0.33 s | **0.011 s** |
| `format(strip_comments=True)` (3.13) | 3.15 s | 2.48 s | **0.043 s** |
| `format(reindent=True, keyword_case='upper')` (3.13) | 3.73 s | 3.04 s | **0.062 s** |
| `format(reindent_aligned=True)` (3.13) | 3.42 s | 2.71 s | **0.052 s** |
| `split_many`, all files (3.13) | 1.02 s | 0.005 s | **0.003 s** |
| `lexer.tokenize`, each file (3.13) | 0.72 s | 0.034 s | 0.032 s |
| `parse`, each file (3.13) | 3.03 s | 2.30 s | **0.18 s** |
| `split`, each file (CPython 3.11) | 0.97 s | 0.76 s | **0.010 s** |
| `format(reindent=True, keyword_case='upper')` (3.11) | 3.65 s | 3.71 s | **0.065 s** |

Its Rust tables follow the Unicode version of the Python it was generated on (3.13), and on every other
Python it hands all non-ASCII text to sqlparse: on 3.11, 243 of the 400 files, and a script that is one of
them as a whole. This package keeps the Rust for all text but the few code points that changed between
Unicode versions (see Unicode versions below).

| What was checked | Result |
| --- | --- |
| Recorded cases (`rust-anything diff`): the original run on the corpus, sqlparse's own test files and generated edge cases, through every option | **7,842 / 7,842 identical**, errors included |
| sqlparse's own test suite, with `sqlparse_turbo.install()` | **537 passed**, same 2 xfail / 1 xpass as plain sqlparse |
| Statement trees (`tools/check_tree.py`): every node's class, value, type and parent, on the corpus, sqlparse's test files, 2,730 cuts of them, 40,000 random scripts and limit inputs | **260,837 calls, 0 differ** |
| Lexer (`tools/check_tokenize.py`): recorded texts, cuts of them, random texts | **32,986 texts, 0 differ** |
| Case filters over every Unicode code point (`tools/check_format_flat.py`) | **26.7 million texts, 0 differ** |
| Package against sqlparse (`tools/check_package.py`): corpus x 13 option sets, split, CLI, bad option types, lexer changes | **6,090 calls in Rust, 0 differ**; 18 out-of-range calls all answered by sqlparse |
| The Rust lexer under `install()`: `lexer.tokenize` and every node of `parse()` (class, token type object, value) on the corpus, sqlparse's test files and 3,000 random scripts; `split_many` against `split` | **3,460 texts, 0 differ**, all lexed in Rust; others (another Lexer, a changed one, bytes, streams) left to sqlparse |
| Lexer and splitter over every Unicode code point (`tools/check_codepoints.py`, `tools/check_split.py`) | **12.2 million and 10.0 million texts, 0 differ** |
| Patched sqlparse (`check_package.py patches`): 6 kinds of patch x format, split, parse; recursion error chain | every patched call answered by sqlparse with the patch, back in Rust once put back; `SQLParseError` from a `RecursionError` as in sqlparse |
| Callers' own filter stacks through `FilterStack.run` (`check_package.py stacks`; `check_recursion.py package`): grouping only, the indent filters, a filter of the caller's own, serializers | same trees and output as sqlparse's run, the recursion error from exactly the same size |
| Other sqlparse versions (`check_other_sqlparse.py`, in CI with 0.4.4, 0.5.3, 0.6.0) | every call answered by that sqlparse, nothing reaches the Rust |
| Django app (`check_django.py`, in CI on every platform and with the other sqlparse versions) | the app turns the Rust on with 0.6.1, `RunSQL`'s split and the debug SQL equal sqlparse's; with another sqlparse nothing replaced and `sqlparse_turbo.W001` |
| Package over every non-ASCII code point (`check_package.py unicode`), on each CPython 3.10-3.14 | the code points that behave as in 3.11 run in Rust, 0 differ; the drifted ones (see Unicode versions) go to sqlparse |
| Fresh random cases (`rust-anything fuzz --time 600`) | **282,550 / 282,556**; the 6 pass `None` where the front door types `str`/`bool` (a known difference of the test bindings, see below) |
| Python's recursion limit (`tools/check_recursion.py`): 11 deep-nesting shapes, 3 caller depths, through C calls | **fails at exactly the same size** on CPython 3.10, 3.11, 3.12, 3.13, 3.14 |
| Platforms, in CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) from the built wheel | Linux x86_64 and aarch64 (manylinux2014), Windows x64, macOS arm64 and Intel (universal2) x **CPython 3.10, 3.11, 3.12, 3.13, 3.14**: all 25 combinations pass sqlparse's suite, the recursion check and the package check (the whole check, front door included, on Linux x86_64) |

## Install

```sh
pip install sqlparse-turbo "sqlparse==0.6.1"   # sqlparse_turbo, the sqlformat-turbo command, and sqlparse 0.6.1
```

Only sqlparse 0.6.1 gets the Rust. The package does not require that version (requiring it would stop a project
pinned to another sqlparse from installing it at all), so pip keeps whatever sqlparse a project already has. On
another version everything works as plain sqlparse, no faster, and the import warns:

```
RuntimeWarning: sqlparse_turbo runs in Rust only with sqlparse 0.6.1; sqlparse 0.5.3 is installed, so every call
goes to sqlparse at its own speed (pip install "sqlparse==0.6.1")
```

`sqlparse_turbo.ACTIVE` tells which it is.

Wheels are built for Linux x86_64 and aarch64, Windows x64 and macOS (arm64 and Intel), one abi3 wheel each for
every CPython from 3.10 on. Elsewhere pip builds from the source distribution, which needs a Rust toolchain.

From a checkout (builds with [maturin](https://www.maturin.rs/)): `pip install .`

or build wheels to copy elsewhere: `maturin build --release` (an abi3 wheel: one file for every CPython from 3.10 on).
`maturin build --release --zig --target x86_64-unknown-linux-gnu --compatibility manylinux2014 -i python3.11`
builds the Linux wheel from Windows.

## Use

`sqlparse_turbo.format(sql, encoding=None, **options)` and `sqlparse_turbo.split(sql, encoding=None,
strip_semicolon=False)` take exactly what sqlparse's take and return exactly what they return, errors
included: an invalid option raises `sqlparse.exceptions.SQLParseError` with sqlparse's message, and a
statement nested past Python's recursion limit raises sqlparse's `SQLParseError('Maximum recursion depth
exceeded')` at the same nesting depth.

`sqlparse_turbo.split_many(sqls, strip_semicolon=False)` is `[sqlparse.split(s) for s in sqls]`, with the texts
split in Rust on all CPU threads at once and Python's lock released (texts the Rust does not take go to
sqlparse one by one). `split` releases the lock too, so other Python threads run meanwhile.

`sqlparse_turbo.install()` replaces `sqlparse.format` and `sqlparse.split` on the module, so every caller that
writes `sqlparse.format(...)` (Django's `prepare_sql_script` and test runner, the `sqlformat` CLI) gets the Rust
without changing a line. It also gives sqlparse's default lexer the Rust tokenizer (`Lexer.get_tokens`), so
`sqlparse.parse()` and everything else that tokenizes gets faster too: the same (token type, value) pairs, with
sqlparse's own token type objects, and only for the default lexer as sqlparse sets it up and a `str` the Rust
takes. A name bound before with `from sqlparse import format` keeps the original. `uninstall()` puts sqlparse's
functions back. `install()` refuses (returns False) if the installed sqlparse is not 0.6.1.

`install()` also replaces `FilterStack.run`, which callers use to run their own sqlparse filters
(django-debug-toolbar's SQL panel): a stack with grouping on and no token-stream filters gets its statements
grouped in Rust, as `parse()` makes them, and runs its own statement filters and serializers on them in Python.
Any other stack runs as sqlparse's.

`sqlformat-turbo` (or `python -m sqlparse_turbo`) is sqlparse's `sqlformat`, same arguments, output and exit codes.

### When the Rust runs, and when sqlparse does

A call runs in Rust only when it is inside what was proven. Everything else goes to sqlparse itself,
unchanged, so the answer is always sqlparse's:

| Runs in Rust | Goes to sqlparse |
| --- | --- |
| sqlparse 0.6.1 installed | any other sqlparse version |
| `sql` is a `str` (any `encoding`: sqlparse only uses it for bytes) | bytes, a stream, a `str` subclass, a `str` with lone surrogates |
| sqlparse's default lexer as sqlparse sets it up | after `Lexer.add_keywords`, `set_SQL_REGEX`, `clear` (Rust again after `default_initialization()`) |
| options of exactly the type the port takes: `True`/`False`, `int`, `str`, `None` where allowed | `reindent=1`, `reindent=None`, `indent_width=4.0`, a `str` subclass, an `int` past 64 bits |
| `indent_width` up to 4096 | a larger `indent_width` (see Limits) |
| text whose characters this Python's Unicode tables treat as CPython 3.11's do (all ASCII text, nearly all other) | text holding a code point that changed since Unicode 14 on this Python (see below) |
| keywords sqlparse never reads (`filename`, `outfile`, ... as the CLI passes them) are dropped, as sqlparse ignores them | |
| sqlparse's code as installed (Rust again once a change is put back) | a replaced, wrapped, added or deleted function, method, class or constant in sqlparse's modules (grouping, filters, `sql`, `tokens`, keywords, lexer, ...) |

The rules are in [`python/sqlparse_turbo/__init__.py`](python/sqlparse_turbo/__init__.py).

**Patched sqlparse.** A program that monkeypatches sqlparse (`grouping.group_identifier = ...`, a new method
on `Identifier`, `MAX_GROUPING_TOKENS = 5`) gets its patch: the package remembers every name in sqlparse's
modules and classes at import and sends calls to sqlparse while one of them differs. On CPython 3.12+ Python
tells the package when one of those dicts changes (a dict watcher), so the comparison runs once after a change
and a call pays well under a microsecond for it; on 3.10 and 3.11 it runs on every call, about 7 µs (a
`format("select 1")` takes ~10 µs there against ~3 µs on 3.13).

**Unicode versions.** The Rust's character tables (`\w`, `\d`, case mapping, `isprintable`, line breaks, final
sigma) are taken from CPython 3.11, which has Unicode 14. Other CPythons ship other Unicode versions: 3.10 has
13.0, 3.12 has 15.0, 3.13 has 15.1, 3.14 has 16.0, and on each some code points behave differently (newly
assigned CJK letters become `\w` and case-insensitive keyword matching changes around them, for example). sqlparse
follows the Python it runs on, so a text holding such a code point goes to sqlparse.
[`tools/unicode_drift.py`](tools/unicode_drift.py) compares every code point's behaviour on each CPython with
3.11's and writes the list ([`_unicode_drift.py`](python/sqlparse_turbo/_unicode_drift.py): 839 code points for
Unicode 13, 4,489 / 5,116 / 10,304 for 15.0 / 15.1 / 16.0). On a Unicode version not in the list, only ASCII text
runs in Rust. The list goes to the Rust once, as one bit per code point, and a text is checked in one pass.

`sqlparse_turbo.parse(sql, encoding=None)` returns what `sqlparse.parse` returns: a tuple of sqlparse's own
`Statement` objects, node for node, for callers to walk and change; `install()` puts it in place of
`sqlparse.parse` (`parsestream`, a generator, stays sqlparse's). It runs in Rust under the same conditions as
`format()`, and raises sqlparse's `SQLParseError` at the same nesting depth.

## How it is proven

The port is checked with [rust-anything](https://github.com/erkamyaman/rust-anything), a tool that records a
Python library's behaviour and replays it against a Rust port:

1. `upstream/` is sqlparse itself, a git submodule pinned to release **0.6.1**. Nothing in it is changed.
2. [`front/sqlparse_front.py`](front/sqlparse_front.py) is the front door: `tokenize`, `split`, `format`
   with every option, and `parse_tree` (`parse()`'s tree as plain lists, so the tree is compared node by node).
3. `rust-anything record` runs the original on generated edge cases and real SQL
   ([`bench/make_inputs.py`](bench/make_inputs.py): the benchmark corpus and sqlparse's own test files, through
   the option sets people use) and saves what every call returns or raises.
4. `rust-anything diff` replays all of it against the Rust (`rust/sqlparse-front/`); `fuzz` generates fresh
   inputs and compares the two live; `gate` runs fmt, clippy, tests, no-`unsafe`, no-panic and the replay.
5. The scripts in [`tools/`](tools) go further than the recording: every Unicode code point through the
   lexer and case filters, hundreds of thousands of trees, the recursion threshold, and the package's public
   calls against sqlparse's.

### How the lexer matches Python's `re`

Python 3.11 and Rust disagree on `\w`, `\s`, `\d`, `\b` and case-insensitive letters outside ASCII (Unicode 14
against a newer table, combining marks, numerics like `½`, `ſ` and the Kelvin sign).
[`tools/gen_lexer.py`](tools/gen_lexer.py) writes `src/lexer_tables.rs` from sqlparse itself: every character
class in `SQL_REGEX` becomes the exact set of code points Python's `re` matches, asked code point by code
point. Patterns with lookahead run on fancy-regex, so backtracking behaves as in `re` (`123abc` lexes as `12`
+ `3abc` in both); one-character lookbehinds are checked on the character before the match. Each rule gets the
set of characters a match can start with, so a position only tries the rules that can match there, and each
rule with lookaround gets a lookaround-free *prefilter* that every match also matches, run first on the
regex crate's DFA: where it finds nothing, the slow backtracking engine is skipped.

### How the tree matches sqlparse's

[`src/tree.rs`](rust/sqlparse-front/src/tree.rs) is `sql.py` on an arena,
[`src/grouping.rs`](rust/sqlparse-front/src/grouping.rs) is `grouping.py` function by function, and
[`src/filters.rs`](rust/sqlparse-front/src/filters.rs) the filters that work on the tree. What sqlparse does by
accident is kept, because output depends on it:

- a group's `value` is joined when the group is made and never refreshed: the reindent filter measures
  identifiers by it, and the Python/PHP output filters print it, after strip_whitespace has changed the
  leaves (`str()` is recomputed from the leaves, as in Python);
- `normalized`, `is_keyword` and `is_whitespace` are taken at creation (`group_operator` later retypes `*`);
- `_group` walks a copy of the list taken at the start, so a token already moved into a group is still looked
  at, with an index that is off; `tidx_offset` and the stale `prev_` are reproduced;
- `ttype in T.X` (subtypes) against `ttype in (T.X, T.Y)` (equality) and `isinstance(x, TokenList)` (any group)
  are told apart everywhere;
- tokens put in with `tokens.insert` have no parent, which `_current_line_len` and `within()` see;
- with an output format, the statement's tokens become a generator the serializer uses up, so the reindent
  filter separates the next statement as if the last one were `''`;
- the limits raise what sqlparse raises: grouping depth 100, 10,000 tokens, `right_margin` is
  `NotImplementedError`, and Python's own recursion limit, which ~980 nested BEGIN blocks or BETWEEN chains
  reach. The port counts the frames sqlparse would use from the caller's real stack depth, asks Python for
  the exact headroom when close to the limit, and knows that CPython 3.12+ no longer counts C calls (such as
  `''.join`) against it, so it fails from exactly the same size as sqlparse on every version.

## Speed

The Rust alone, no Python (`cargo run --release --example bench_format`, `bench_lexer`, `bench_split`), same
corpus, best of 3, Windows 11, after each optimisation pass:

| Step | first port | regex lexer | fast lexer (0.1.1) | **0.1.2** |
| --- | ---: | ---: | ---: | ---: |
| lex (`tokenize`, 799k tokens) | 0.19 s | 0.087 s | 0.026 s | **0.021 s** |
| `split` | 0.21 s | 0.10 s | 0.032 s | **0.026 s** |
| `parse` (lex, split, group) | 0.50 s | 0.36 s | 0.30 s | **0.126 s** |
| `format(reindent=True, keyword_case='upper')` | 0.62 s | 0.48 s | 0.41 s | **0.201 s** |
| `format(strip_comments=True)` | 0.49 s | 0.37 s | 0.28 s | **0.147 s** |
| `format(reindent_aligned=True)` | 0.55 s | 0.41 s | 0.34 s | **0.178 s** |

0.1.2 went after grouping and memory, measured pass by pass and with a profiler, every change checked equal:

- **Passes that cannot do anything are left out.** sqlparse runs all 25 grouping passes over every statement;
  most statements have no `CASE`, `::`, `:=`, `OVER`, `[`, comment or `VALUES`, so the passes that start from
  those find nothing. A pass is skipped only when none of the tokens it starts from is in the statement *and*
  it could not have stopped on a limit either: sqlparse's grouping depth (100) and list length (10,000), which
  every `_group` pass checks on every list, and Python's recursion limit for the passes that count frames, so
  errors stay exactly where sqlparse raises them.
- **Fewer allocations.** Tokens are no longer copied into a string each before the tree is built; passes walk
  a copy of each token list taken in one shared buffer instead of a new vector per list and pass; a node's
  own strings live in one list per tree, which shrinks a node from about 130 to 80 bytes; the indent filters'
  keyword patterns no longer collect a keyword's characters fourteen times per look. And the module uses
  [mimalloc](https://github.com/microsoft/mimalloc) for the Rust's own allocations (6-14 % on Linux, 17-21 % on
  Windows, whose allocator is slow).
- **Token types are numbers.** A token's type was compared as a string (`Token.Keyword.DML`); it is now a
  number into the list of types, and `ttype in T.Keyword` is one bit in a table built at compile time.
- **Keywords are found by hash**, not by a binary search comparing about ten strings per word.

The step before was the lexer. 88 % of tokens (spaces, words, punctuation, numbers, strings, comparison
operators) are now recognised by hand-written matchers that decide sqlparse's rules in sqlparse's order:
`fast_token` in [`src/lexer.rs`](rust/sqlparse-front/src/lexer.rs). Tests pin which rules each character can
start, and every other case still runs the regexes: a non-ASCII character where `\w` or `\d` would matter,
`$`/`#` in a word, an unclosed string, a word that can open a multi-word keyword (`GROUP BY`, `LEFT JOIN`; then
only that keyword's own rules run). The splitter stopped allocating for keywords and slices statements straight
out of the text.

The package adds little on top (the string crosses into Rust and back once per call, plus a look at the
Python stack for the recursion limit).

No single hot spot is left: the time is spread over the grouping passes that do run, the lexer and the indent
filters. Going further would mean merging grouping passes, which sqlparse runs one after another over the whole
tree and whose order the output depends on.

What made it fast besides being Rust: tokens are spans into one string, the tree is an arena of indices,
grouping passes walk the tree instead of rebuilding lists, keyword lookups use a sorted table without
allocating, and the lexer skips most regex work (first-character table, DFA prefilters for lookaround rules:
the last pass made the lexer 2.2x faster with them).
sqlparse spends 58 % of `format()` in grouping, 27 % lexing and splitting and 9 % in the reindent filter
([`docs/profile-format.md`](docs/profile-format.md)); its quadratic spots (`_current_line_len` re-joining the
whole statement for every line) are walked backwards to the last line break in the port.

## Limits and known differences

- **Out of memory.** sqlparse raises `MemoryError`; Rust cannot catch a failed allocation and stops the
  process. The indent string, the one thing that grows with an option rather than with the input, is
  allocated as Python does it (`OverflowError` past `sys.maxsize`, `MemoryError` when it cannot be had), but a
  later copy of a gigabyte-sized result can still fail. Only a huge `indent_width` makes a small statement
  that big, so the package gives `indent_width` above 4096 to sqlparse.
- **A call the package hands to sqlparse** has the package's frame on the stack as well, so sqlparse's own
  recursion limit comes one nesting level sooner than when called directly.
- **`FilterStack.run` with `install()`** decides between the Rust and sqlparse's own run when it is called;
  sqlparse's reads the stack's filters when the first statement is asked for, so a filter added to the stack
  after `run()` was called but before iterating is not seen.
- **The `RecursionError` behind the recursion `SQLParseError`** says `maximum recursion depth exceeded`. On
  CPython 3.10 sqlparse's says where Python hit the limit as well (`... in comparison`, `... in
  __instancecheck__`), which depends on the C call sqlparse was in; the `SQLParseError` itself is the same.
- **The front-door bindings** (`rust/sqlparse-front`, used for the recording) take typed arguments: `None`
  for a bool option is a `TypeError` there, where sqlparse treats it as False. The package never sends such
  calls to the Rust.
- **Exception class** needs rust-anything with dotted exception kinds
  ([erkamyaman/rust-anything#207](https://github.com/erkamyaman/rust-anything/pull/207), not merged yet) to
  regenerate the bindings; the bindings in this repository already raise sqlparse's own `SQLParseError`.
- **Not tested:** Windows on ARM, musl (Alpine), PyPy, free-threaded CPython; no wheels are built for them.

## Layout

| Path | What |
| --- | --- |
| `python/sqlparse_turbo/` | the package: `format`, `split`, `install`, the CLI; `django/` the Django app |
| `rust/sqlparse-front/src/` | the port: `lexer.rs`, `split.rs`, `tree.rs`, `grouping.rs`, `filters.rs`, `format.rs`, `port.rs` (about 3,800 hand-written lines); `lexer_tables.rs`, `pyunicode_tables.rs` are generated from Python by `tools/gen_lexer.py` and `tools/gen_unicode.py`; `lib.rs` and the conversions are generated by rust-anything |
| `front/` | the front door rust-anything records and replays |
| `upstream/` | sqlparse 0.6.1 (git submodule) |
| `bench/` | corpus generator (`gen_corpus.py`, same seed = same bytes), Python workload, recording inputs, `bench_package.py` |
| `tools/` | the checks listed above, generators, `run_upstream_package.py` |

## Development

```sh
git submodule update --init
python -m venv .venv && .venv/Scripts/python -m pip install -e ./upstream pytest maturin
.venv/Scripts/python -X utf8 bench/gen_corpus.py          # the corpus (not committed)
.venv/Scripts/python -X utf8 bench/make_inputs.py         # inputs for the recording

# the port, with .venv/Scripts first on PATH so rust-anything's Python sees the pinned sqlparse:
rust-anything record rust/sqlparse-front --inputs rust/sqlparse-front/inputs.jsonl
rust-anything build rust/sqlparse-front
rust-anything diff rust/sqlparse-front
rust-anything gate rust/sqlparse-front
cargo run --release --manifest-path rust/sqlparse-front/Cargo.toml --example bench_format   # Rust alone

# the package:
VIRTUAL_ENV=.venv maturin develop --release
.venv/Scripts/python -X utf8 tools/run_upstream_package.py
.venv/Scripts/python -X utf8 tools/check_package.py
.venv/Scripts/python -X utf8 tools/check_recursion.py package
.venv/Scripts/python -X utf8 bench/bench_package.py
```

`rust-anything gate` passes everything except `size`: it compares the Rust's line count with the 59-line
front door, and most of the Rust is the generated Unicode and lexer tables.

## License

BSD-3-Clause, like sqlparse ([`LICENSE`](LICENSE)). The port follows sqlparse's code closely, so sqlparse's
copyright notice is kept next to ours.
