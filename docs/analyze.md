# Rust migration plan: upstream (Python)

1 package, 27 source files (4022 lines), 11 test files, 0 UI files, grouped into 4 units.

| Ready | Caution | Blocked | Keep in source language |
| ---: | ---: | ---: | ---: |
| 2 | 2 | 0 | 0 |

## Start here

Best first ports: safe to port (readiness) and worth porting (loops and string work, tests, size).

| # | Unit | Lines | Priority | Readiness | Tests | Bridge | Main risks |
| ---: | --- | ---: | ---: | ---: | ---: | --- | --- |
| 1 | `benchmarks/` | 341 | 63 | 63 | 0 | PyO3 | String offsets and slicing (6); Dynamic typing (19); Class inheritance (1) |
| 2 | `sqlparse/engine/` | 640 | 51 | 64 | 0 | PyO3 | Dynamic typing (81); Generators (6); Exceptions (7) |
| 3 | `sqlparse/filters/` | 751 | 43 | 39 | 0 | PyO3 | String offsets and slicing (6); Generators (78); Dynamic typing (55) |
| 4 | `sqlparse/` | 2290 | 25 | 0 | 10 | PyO3 | String offsets and slicing (16); Regex features the `regex` crate lacks (3); Runtime changes to code (3) |

### `benchmarks/`

341 lines in 6 files, wave 1, priority 63, readiness 63, bridge **PyO3**.

- 1 PyPI dependencies not in the crate table: check them by hand.
- String offsets and slicing x6: `benchmarks/_harness.py:121`, `benchmarks/bench_group_comments.py:37`, `benchmarks/bench_grouping.py:51`
- Dynamic typing x19: `benchmarks/_harness.py:71`, `benchmarks/_harness.py:77`, `benchmarks/_harness.py:82`
- Class inheritance x1: `benchmarks/_harness.py:67`
- Integer width and overflow x1: `benchmarks/_harness.py:126`
- Division and modulo x1: `benchmarks/_harness.py:121`
- Exceptions x2: `benchmarks/_harness.py:72`, `benchmarks/_harness.py:92`

### `sqlparse/engine/`

640 lines in 4 files, wave 0, priority 51, readiness 64, bridge **PyO3**.

- Import cycle with sqlparse/filters/, sqlparse/: port them together.
- Dynamic typing x81: `sqlparse/engine/filter_stack.py:29`, `sqlparse/engine/grouping.py:31`, `sqlparse/engine/grouping.py:58`
- Generators x6: `sqlparse/engine/filter_stack.py:49`, `sqlparse/engine/filter_stack.py:49`, `sqlparse/engine/statement_splitter.py:162`
- Exceptions x7: `sqlparse/engine/filter_stack.py:30`, `sqlparse/engine/filter_stack.py:51`, `sqlparse/engine/grouping.py:34`

### `sqlparse/filters/`

751 lines in 7 files, wave 0, priority 43, readiness 39, bridge **PyO3**.

- Import cycle with sqlparse/engine/, sqlparse/: port them together.
- String offsets and slicing x6: `sqlparse/filters/reindent.py:52`, `sqlparse/filters/reindent.py:52`, `sqlparse/filters/tokens.py:50`
- Generators x78: `sqlparse/filters/output.py:38`, `sqlparse/filters/output.py:38`, `sqlparse/filters/output.py:39`
- Dynamic typing x55: `sqlparse/filters/aligned_indent.py:34`, `sqlparse/filters/aligned_indent.py:41`, `sqlparse/filters/aligned_indent.py:49`
- Class inheritance x4: `sqlparse/filters/output.py:34`, `sqlparse/filters/output.py:80`, `sqlparse/filters/tokens.py:25`
- Exceptions x2: `sqlparse/filters/output.py:20`, `sqlparse/filters/right_margin.py:49`

### `sqlparse/`

2290 lines in 10 files, wave 0, priority 25, readiness 0, bridge **PyO3**.

- Import cycle with sqlparse/filters/, sqlparse/engine/: port them together.
- 10 test files import it directly.
- String offsets and slicing x16: `sqlparse/lexer.py:148`, `sqlparse/sql.py:93`, `sqlparse/sql.py:313`
- Regex features the `regex` crate lacks x3: `sqlparse/keywords.py:33`, `sqlparse/keywords.py:35`, `sqlparse/keywords.py:37`
- Runtime changes to code x3: `sqlparse/tokens.py:64`, `sqlparse/tokens.py:65`, `sqlparse/tokens.py:66`
- Regexes built at runtime x2: `sqlparse/lexer.py:84`, `sqlparse/sql.py:122`
- Dynamic typing x64: `sqlparse/cli.py:166`, `sqlparse/cli.py:172`, `sqlparse/formatter.py:14`
- Generators x24: `sqlparse/lexer.py:148`, `sqlparse/lexer.py:148`, `sqlparse/lexer.py:158`
- Class inheritance x23: `sqlparse/exceptions.py:11`, `sqlparse/sql.py:158`, `sqlparse/sql.py:411`
- Reflection and prototypes x2: `sqlparse/cli.py:197`, `sqlparse/sql.py:85`
- Exceptions x32: `sqlparse/cli.py:184`, `sqlparse/cli.py:189`, `sqlparse/cli.py:198`
- Number semantics x7: `sqlparse/__init__.py:21`, `sqlparse/__init__.py:33`, `sqlparse/__init__.py:33`

## Order by package

Packages come after the workspace packages they use. Inside a package, port by wave: a unit's imports are in earlier waves.

### 1. sqlparse (`.`, portable, 11 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `sqlparse/engine/` | ready | 640 | 64 | PyO3 | Dynamic typing (81); Generators (6) |
| 0 | `sqlparse/filters/` | caution | 751 | 39 | PyO3 | String offsets and slicing (6); Generators (78) |
| 0 | `sqlparse/` | caution | 2290 | 0 | PyO3 | String offsets and slicing (16); Regex features the `regex` crate lacks (3) |
| 1 | `benchmarks/` | ready | 341 | 63 | PyO3 | String offsets and slicing (6); Dynamic typing (19) |

## PyPI dependencies

| Package | Units | Rust | Fit | Note |
| --- | ---: | --- | --- | --- |
| `-harness` | 1 | ? | unknown |  |

## Risk guide

- **String offsets and slicing** (High): JS indexes strings in UTF-16 code units, Rust in UTF-8 bytes. Offsets that cross the boundary must be converted, or non-ASCII text gets wrong ranges silently.
- **Regex features the `regex` crate lacks** (High): Lookaround, backreferences and the `y` flag are not in the `regex` crate. Use `fancy-regex` for those patterns, or rewrite them, and test them on non-ASCII text: `\w`, `\b` and case-insensitive matching differ between engines even where a pattern compiles. JS `/i` without `u` never matches a non-ASCII letter to an ASCII one, Rust `(?i)` does (`ſ` matches `s`, the Kelvin sign `K` matches `k`): write `(?i-u)` or compare with `eq_ignore_ascii_case`.
- **Regexes built at runtime** (High): Patterns come from data or config in JS syntax. Rust regex syntax differs, so user-visible behavior can change. Pin it with tests on real patterns.
- **Dynamic typing** (Medium): Values whose shape is checked at runtime need an explicit Rust enum or `serde_json::Value`.
- **Exceptions** (Low): Thrown errors become `Result`. Errors crossing the bridge must map back to the same JS error shape.
- **Generators** (Medium): Lazy generators become iterators, or return a collected Vec across the bridge.
- **Class inheritance** (Medium): Class hierarchies become traits plus composition; `instanceof` checks become enums.
- **Reflection and prototypes** (Medium): Proxy, Reflect, Symbol and prototype tricks need a redesign, not a translation.
- **Number semantics** (Low): JS numbers are f64; bitwise ops truncate to i32; parseInt is lenient. Pick Rust integer types deliberately. Printing differs: `${n}` gives `1e+21`, `1e-7` and `0` for -0 where Rust's `{}` does not, so use `js::number_to_string`. `Math.round` rounds halves up (-2.5 gives -2), Rust's `round` away from zero: use `js::math_round`. `toFixed` rounds exact halves up ((2.5).toFixed(0) is "3"), Rust's `{:.0}` to even.
- **Integer width and overflow** (Medium): Python ints never overflow; C, Go and Java ints wrap or are undefined on overflow. Pick i64, i128 or a bignum crate deliberately, and use wrapping_* where the source wraps.
- **Division and modulo** (Medium): Python `//` and `%` floor toward negative infinity; Rust `/` and `%` truncate: use `div_euclid`/`rem_euclid` or explicit floor math. C division by zero is undefined.
- **Runtime changes to code** (High): Code that rewrites classes or modules at runtime has no Rust equivalent: keep it in the source language and port the stable parts.
