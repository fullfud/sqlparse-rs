# Changelog

Every version runs in Rust with sqlparse 0.6.1 and gives sqlparse's answers. Each change below was checked equal to
sqlparse before release ([Verification](verification.md)).

## 0.1.8 (2026-11-24)

- **`str()` of a statement in Rust** on CPython 3.12+: sqlparse's `TokenList.__str__` reads the leaves straight
  from the objects (checked by `tools/check_str.py`). `[str(s) for s in parsestream(...)]` 18.6x → 23x.
- **A caller's own stack with sqlparse's `SerializerUnicode`** after an `AlignedIndentFilter` or `ReindentFilter`
  gets the string from the Rust: `AlignedIndentFilter` 18x → 66x (no Python objects at all), `ReindentFilter`
  15.5x → 22x on the corpus, 20x → 30x on a short query.

## 0.1.7 (2026-11-24)

- **django-debug-toolbar's SQL panel 62x faster** (was 5.8x): its own filters (`ElideSelectListsFilter`,
  `BoldKeywordFilter`, `EscapedStringSerializer`) run in Rust too, for both the formatted and the collapsed
  SQL, when they are the code django-debug-toolbar 4.1 to 8.0 ship. A page of 200 queries through the toolbar
  loads 3.3x faster (was 2.4x). Checked in CI by `tools/check_toolbar.py`.

## 0.1.6 (2026-11-24)

- Documentation only, the code is 0.1.5's: a short README with pictures (also on PyPI), the details in
  [docs/](README.md), one page per subject, and `bench/quick_speed.py` to measure the speed-up yourself.

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
