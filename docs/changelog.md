# Changelog

Every version runs in Rust with sqlparse 0.6.1 and gives sqlparse's answers. Each change below was checked equal to
sqlparse before release ([Verification](verification.md)).

## 0.1.5 (2026-11-24)

- **Memory.** `FilterStack.run` and `parsestream` lex one statement at a time instead of the whole text first.
  `parsestream` over 100,000 statements peaks at 59 MB, sqlparse at 56 MB (0.1.4: 410 MB).
- `format` and `split` run as fast as in 0.1.4.
- The published wheels are built with a pinned maturin (v1.16.0).

## 0.1.4 (2026-11-24)

- **Bytes and text streams** are read and decoded as sqlparse does, then run in Rust: `format(bytes)` 87x.
- **`FilterStack.run`** gives statements one at a time, as sqlparse's does. A leading `AlignedIndentFilter` or
  `ReindentFilter` runs in Rust: a caller's own stack with a `ReindentFilter` 15.5x, django-debug-toolbar's panel
  5.8x, `parsestream` 8.6x.
- **`parse`** builds sqlparse's objects by writing their slots directly, with the garbage collector paused:
  0.13 → 0.10 s on the corpus.
- **Cheaper checks per call**: tiny `format` 2.7 → 2.2 µs.
- **Near the recursion limit**, sqlparse's code runs in the package's own frame, so it fails where sqlparse's
  would.
- **Indent budget**: past 256 MiB of indent in one call, the call goes to sqlparse.
- CI actions pinned to commit SHAs.

## 0.1.3 (2026-11-23)

- **Django app**: `"sqlparse_turbo.django"` in `INSTALLED_APPS` turns the Rust on; check `W001` when it cannot.

## 0.1.2 (2026-11-23)

- **Core 2.3x faster on `parse`, 2x on `format`**: grouping passes with nothing to do are skipped, fewer
  allocations, token types as numbers, keywords by hash, mimalloc ([details](internals.md#012-grouping-and-memory)).
- The linux-aarch64 wheel is built with zig.

## 0.1.1 (2026-11-23)

- **Works with any sqlparse from 0.4**: other versions get sqlparse's own answers, the import warns, and
  `sqlparse_turbo.ACTIVE` tells which it is.
- `install()` groups `FilterStack.run` in Rust (django-debug-toolbar).
- The race animation and the speed chart in the README.

## 0.1.0 (2026-11-23)

- First release: `format`, `split`, `split_many` and `parse` in Rust; `install()`; the `sqlformat-turbo` command.
- Calls go to sqlparse when it is monkeypatched, when the text holds Unicode-drifted code points, and for every
  other input outside what was proven.
- Wheels for Linux x86_64 and aarch64, Windows x64 and macOS universal2, CPython 3.10-3.14.
