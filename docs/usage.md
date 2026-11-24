# Usage

```python
import sqlparse_turbo

sqlparse_turbo.format(sql, reindent=True, keyword_case='upper')   # == sqlparse.format(sql, reindent=True, ...)
sqlparse_turbo.split(sql)                                          # == sqlparse.split(sql)
sqlparse_turbo.split_many(sqls)                                    # split() of each, on all CPU threads
sqlparse_turbo.parse(sql)                                          # == sqlparse.parse(sql): sqlparse's own objects

sqlparse_turbo.install()    # from now on sqlparse.format / sqlparse.split are the Rust ones, for every caller,
                            # and sqlparse.parse and sqlparse's lexer too
```

Every call answers exactly as sqlparse does. A call the Rust does not take goes to sqlparse itself
([When the Rust runs](fallback.md)).

## `format` and `split`

`sqlparse_turbo.format(sql, encoding=None, **options)` and `sqlparse_turbo.split(sql, encoding=None,
strip_semicolon=False)` take exactly what sqlparse's take and return exactly what they return, errors included:

- an invalid option raises `sqlparse.exceptions.SQLParseError` with sqlparse's message;
- a statement nested past Python's recursion limit raises sqlparse's `SQLParseError('Maximum recursion depth
  exceeded')` at the same nesting depth.

`split` releases Python's lock, so other Python threads run meanwhile.

## `split_many`

`sqlparse_turbo.split_many(sqls, strip_semicolon=False)` is `[sqlparse.split(s) for s in sqls]`. The texts are
split in Rust on all CPU threads at once, with Python's lock released. Texts the Rust does not take go to
sqlparse one by one.

## `parse`

`sqlparse_turbo.parse(sql, encoding=None)` returns what `sqlparse.parse` returns: a tuple of sqlparse's own
`Statement` objects, node for node, for callers to walk and change.

The statements are grouped in Rust and handed back as sqlparse's own `Statement`, `Identifier`, ... objects. They
are made with `object.__new__` and their slots written from the Rust tree. So a group's value joined when the
group was made, `normalized` taken at creation and every `parent` pointer are sqlparse's.

It runs in Rust under the same conditions as `format()`, and raises sqlparse's `SQLParseError` at the same
nesting depth.

## `install()` and `uninstall()`

`sqlparse_turbo.install()` replaces `sqlparse.format`, `sqlparse.split` and `sqlparse.parse` on the module. Every
caller that writes `sqlparse.format(...)` then gets the Rust without changing a line: Django's
`prepare_sql_script` and test runner, the `sqlformat` CLI, your own code.

It also gives sqlparse's default lexer the Rust tokenizer (`Lexer.get_tokens`), so everything else that
tokenizes gets faster too. The tokenizer gives the same (token type, value) pairs, with sqlparse's own token type
objects. It runs only for the default lexer as sqlparse sets it up and a `str` the Rust takes.

- A name bound before with `from sqlparse import format` keeps the original.
- `uninstall()` puts sqlparse's functions back.
- `install()` refuses (returns False) if the installed sqlparse is not 0.6.1.

## `FilterStack.run`: callers' own filters

`install()` also replaces `FilterStack.run`, which callers use to run their own sqlparse filters
(django-debug-toolbar's SQL panel does).

- A stack with grouping on and no token-stream filters gets its statements grouped in Rust, as `parse()` makes
  them, one at a time as sqlparse's run does.
- A first statement filter that is sqlparse's own `AlignedIndentFilter` or `ReindentFilter` runs in Rust too,
  from the instance's state. The instance is left as sqlparse's would be (`_last_stmt`, `_last_func`).
- The stack's other statement filters and its serializers run in Python.
- Any other stack runs as sqlparse's.

## `parsestream`

`sqlparse.parsestream` goes through `FilterStack.run`, so with `install()` its statements come from the Rust too.
Since 0.1.5 they are lexed and split off as they are asked for, so iterating holds about what sqlparse holds:
the text and the statement being made ([memory](benchmarks.md#memory)).

## Bytes and streams

`sql` may be bytes or a text stream wherever sqlparse takes one. The package reads and decodes it as sqlparse's
lexer does: the same calls on the stream, the same encodings tried, the same errors. Then it runs the text in
Rust.

## Command line

```sh
sqlformat-turbo -r -k upper query.sql      # or: python -m sqlparse_turbo -r -k upper query.sql
```

`sqlformat-turbo` is sqlparse's `sqlformat`: same arguments, output and exit codes.
