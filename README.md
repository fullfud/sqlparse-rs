<h1 align="center">sqlparse-turbo</h1>

<p align="center">
  <a href="https://pypi.org/project/sqlparse-turbo/"><img src="https://img.shields.io/pypi/v/sqlparse-turbo?color=FF8A3D&label=PyPI" alt="PyPI version"></a>
  <a href="https://pypi.org/project/sqlparse-turbo/"><img src="https://img.shields.io/pypi/pyversions/sqlparse-turbo" alt="Python versions"></a>
  <a href="https://github.com/fullfud/sqlparse-turbo/actions/workflows/ci.yml"><img src="https://github.com/fullfud/sqlparse-turbo/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/Linux%20·%20macOS%20·%20Windows-wheels-2563EB" alt="Wheels for Linux, macOS and Windows">
  <a href="https://github.com/fullfud/sqlparse-turbo/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-BSD--3--Clause-555" alt="BSD-3-Clause license"></a>
</p>

<p align="center"><b><a href="https://github.com/andialbrecht/sqlparse">sqlparse</a>'s <code>format()</code>,
<code>split()</code> and <code>parse()</code>, rewritten in Rust:<br>same output, byte for byte, 30-150 times
faster.</b></p>

<p align="center">
  <img src="https://raw.githubusercontent.com/fullfud/sqlparse-turbo/main/docs/assets/race.gif" alt="sqlparse.format(reindent=True, keyword_case='upper') over 400 SQL queries, each file at its measured time: sqlparse takes 3.61 s, sqlparse-turbo 0.053 s (69x), and both give the same output (same sha256)." width="820">
</p>

```sh
pip install sqlparse-turbo "sqlparse==0.6.1"
```

> [!IMPORTANT]
> **The speed-up needs sqlparse 0.6.1.** With any other sqlparse every call still works, at sqlparse's own speed:
> `sqlparse_turbo.ACTIVE` is False and the import warns. [Why][install]

## Use

```python
import sqlparse_turbo

sqlparse_turbo.format(sql, reindent=True, keyword_case='upper')   # == sqlparse.format(sql, reindent=True, ...)
sqlparse_turbo.split(sql)                                          # == sqlparse.split(sql)
sqlparse_turbo.split_many(sqls)                                    # split() of each, on all CPU threads
sqlparse_turbo.parse(sql)                                          # == sqlparse.parse(sql): sqlparse's own objects

sqlparse_turbo.install()    # sqlparse.format / split / parse and sqlparse's lexer become the Rust ones,
                            # for every caller
```

```sh
sqlformat-turbo -r -k upper query.sql      # sqlparse's own `sqlformat` command, formatting in Rust
```

**Django:** add `"sqlparse_turbo.django"` to `INSTALLED_APPS`. Migrations, `sqlmigrate`, the test runner and
django-debug-toolbar then run in Rust; the toolbar's SQL panel is 5.8x faster. [More][django]

Everything else, `install()`, `FilterStack.run`, bytes and streams: [Usage][usage].

## Speed

<p align="center">
  <img src="https://raw.githubusercontent.com/fullfud/sqlparse-turbo/main/docs/assets/speed.svg" alt="Seconds for 400 SQL queries, one call per file, Python 3.13: split 0.97 s against 0.0065 s (150x), format with keyword_case 1.00 s against 0.0092 s (109x), format with reindent 3.61 s against 0.053 s (69x), parse 2.92 s against 0.097 s (30x)." width="760">
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/fullfud/sqlparse-turbo/main/docs/assets/calls.svg" alt="Speed-ups with install(): format of bytes 87x, format of one short ORM query 58x, split of one short query 42x, FilterStack.run with a ReindentFilter 15.5x, parsestream 8.6x, django-debug-toolbar's SQL panel 5.8x." width="760">
</p>

Measure it on your machine in a few seconds with [`bench/quick_speed.py`][quick]:

```
$ python bench/quick_speed.py
                              sqlparse       turbo    faster
format(reindent, upper)         145 ms      1.5 ms      100x
split                            38 ms      0.2 ms      213x
parse                           115 ms      3.9 ms       29x
```

The whole corpus, memory, against markocerkez/sqlparse-rs, how each number was made: [Benchmarks][benchmarks].

## How it works

<p align="center">
  <img src="https://raw.githubusercontent.com/fullfud/sqlparse-turbo/main/docs/assets/how-it-works.svg" alt="Every call is checked first: when every check passes it runs in the Rust port, when any fails it goes to sqlparse 0.6.1 itself, unchanged; either way the output and errors are sqlparse's." width="760">
</p>

sqlparse is the pure-Python SQL splitter and formatter Django depends on, and it is slow: two megabytes of SQL
take over ten seconds to format. Rust SQL libraries are fast but format differently. This is sqlparse 0.6.1
itself ported to Rust, quirks included, so the output does not change. [When the Rust runs][fallback] ·
[Internals][internals]

## Proven the same

<p align="center">
  <img src="https://raw.githubusercontent.com/fullfud/sqlparse-turbo/main/docs/assets/proof.svg" alt="Checked against sqlparse itself: 7,842 of 7,842 recorded calls give sqlparse's answer, sqlparse's own test suite passes (537), 260,837 trees compared node by node, 26.7 million texts over every Unicode code point, 25 of 25 platform and Python jobs in CI, and the recursion error at sqlparse's nesting depth on 3.10-3.14." width="760">
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/fullfud/sqlparse-turbo/main/docs/assets/platforms.svg" alt="Wheels and CI: Linux x86_64, Linux aarch64, Windows x64, macOS arm64 and macOS Intel, each with CPython 3.10 to 3.14, all 25 jobs pass." width="760">
</p>

Every check and how it runs: [Verification][verification]. What is known to differ: [Limits][limits].

## Documentation

| Page | What is in it |
| --- | --- |
| [Install][install] | why sqlparse 0.6.1, the warning on other versions, wheels, building from source |
| [Usage][usage] | every function, `install()`, `FilterStack.run`, bytes and streams, `parsestream`, the CLI |
| [Django][django] | the app, check `W001`, django-debug-toolbar, migrations, tests |
| [When the Rust runs][fallback] | which calls go to sqlparse instead, patched sqlparse, Unicode versions |
| [Benchmarks][benchmarks] | every measurement, memory, against markocerkez/sqlparse-rs |
| [Verification][verification] | how the port is proven to give sqlparse's answers |
| [Internals][internals] | the lexer, the tree's quirks, what made it fast |
| [Limits][limits] | known differences and what is not tested |
| [Development][development] | layout, building, the checks, releases |
| [Changelog][changelog] | what each version changed |

## License

BSD-3-Clause, like sqlparse ([`LICENSE`][license]). The port follows sqlparse's code closely, so sqlparse's
copyright notice is kept next to ours.

[install]: https://github.com/fullfud/sqlparse-turbo/blob/main/docs/install.md
[usage]: https://github.com/fullfud/sqlparse-turbo/blob/main/docs/usage.md
[django]: https://github.com/fullfud/sqlparse-turbo/blob/main/docs/django.md
[fallback]: https://github.com/fullfud/sqlparse-turbo/blob/main/docs/fallback.md
[benchmarks]: https://github.com/fullfud/sqlparse-turbo/blob/main/docs/benchmarks.md
[verification]: https://github.com/fullfud/sqlparse-turbo/blob/main/docs/verification.md
[internals]: https://github.com/fullfud/sqlparse-turbo/blob/main/docs/internals.md
[limits]: https://github.com/fullfud/sqlparse-turbo/blob/main/docs/limits.md
[development]: https://github.com/fullfud/sqlparse-turbo/blob/main/docs/development.md
[changelog]: https://github.com/fullfud/sqlparse-turbo/blob/main/docs/changelog.md
[quick]: https://github.com/fullfud/sqlparse-turbo/blob/main/bench/quick_speed.py
[license]: https://github.com/fullfud/sqlparse-turbo/blob/main/LICENSE
