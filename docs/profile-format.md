440 files, 2.1 MB
format    72.63 s
total     72.63 s
# Profile: `../.venv/Scripts/python ../bench/workload.py ../bench/corpus --only format`

72802 ms in all; 83% of it in the repository's own code.

| Function | File | Total | Self | Calls | Unit |
| --- | --- | ---: | ---: | ---: | --- |
| `<module>` | `../bench/workload.py:1` | 100% (72700 ms) | 0 ms | 1 | - |
| `main` | `../bench/workload.py:34` | 100% (72673 ms) | 2 ms | 1 | - |
| `run` | `../bench/workload.py:16` | 100% (72632 ms) | 0 ms | 1 | - |
| `timed` | `../bench/workload.py:19` | 100% (72632 ms) | 1 ms | 3 | - |
| `<lambda>` | `../bench/workload.py:28` | 100% (72631 ms) | 2 ms | 440 | - |
| `format` | `sqlparse/__init__.py:46` | 100% (72629 ms) | 3 ms | 880 | `sqlparse/` |
| `run` | `sqlparse/engine/filter_stack.py:29` | 100% (72610 ms) | 17 ms | 5156 | `sqlparse/engine/` |
| `group` | `sqlparse/engine/grouping.py:446` | 58% (42117 ms) | 48 ms | 4276 | `sqlparse/engine/` |
| `process` | `sqlparse/engine/statement_splitter.py:151` | 27% (19927 ms) | 1972 ms | 5156 | `sqlparse/engine/` |
| `_token_matching` | `sqlparse/sql.py:235` | 27% (19927 ms) | 5472 ms | 3458675 | `sqlparse/` |
| `token_next_by` | `sqlparse/sql.py:272` | 27% (19465 ms) | 783 ms | 2393251 | `sqlparse/` |
| `wrapped_f` | `sqlparse/utils.py:74` | 26% (19079 ms) | 904 ms | 1074590 | `sqlparse/` |
| `_group` | `sqlparse/engine/grouping.py:495` | 22% (15875 ms) | 6419 ms | 2164030 | `sqlparse/engine/` |
| `<lambda>` | `sqlparse/sql.py:274` | 19% (13828 ms) | 2943 ms | 16384002 | `sqlparse/` |
| `imt` | `sqlparse/utils.py:85` | 19% (13719 ms) | 6192 ms | 19535456 | `sqlparse/` |
| `get_tokens` | `sqlparse/lexer.py:106` | 18% (13183 ms) | 6080 ms | 1599742 | `sqlparse/` |
| `process` | `sqlparse/filters/tokens.py:18` | 10% (7240 ms) | 348 ms | 799871 | `sqlparse/filters/` |
| `_group_matching` | `sqlparse/engine/grouping.py:31` | 9% (6827 ms) | 3463 ms | 295506 | `sqlparse/engine/` |
| `match` | `sqlparse/sql.py:100` | 9% (6761 ms) | 3911 ms | 26919650 | `sqlparse/` |
| `process` | `sqlparse/filters/reindent.py:279` | 9% (6737 ms) | 5 ms | 2138 | `sqlparse/filters/` |
| `_process` | `sqlparse/filters/reindent.py:140` | 9% (6271 ms) | 116 ms | 156211 | `sqlparse/filters/` |
| `_process_default` | `sqlparse/filters/reindent.py:273` | 9% (6268 ms) | 161 ms | 155940 | `sqlparse/filters/` |
| `group_typed_literal` | `sqlparse/engine/grouping.py:138` | 6% (4274 ms) | 6 ms | 4276 | `sqlparse/engine/` |
| `_process_identifierlist` | `sqlparse/filters/reindent.py:170` | 6% (4201 ms) | 61 ms | 8328 | `sqlparse/filters/` |
| `_process_parenthesis` | `sqlparse/filters/reindent.py:154` | 6% (4036 ms) | 64 ms | 21231 | `sqlparse/filters/` |

## What to port

- **`format`** (100%): Hot inside `sqlparse/`: port the exported function that calls it, with it.
- **`run`** (100%): Hot inside `sqlparse/engine/`: port the exported function that calls it, with it.
- **`group`** (58%): Hot and exported by `sqlparse/engine/`: port it (`rust-anything bridge upstream sqlparse/engine/`), then check `rust-anything bench`.
- **`process`** (27%): Hot inside `sqlparse/engine/`: port the exported function that calls it, with it.
- **`_token_matching`** (27%): Hot inside `sqlparse/`: port the exported function that calls it, with it.
- **`token_next_by`** (27%): Hot inside `sqlparse/`: port the exported function that calls it, with it.
- **`wrapped_f`** (26%): Hot inside `sqlparse/`: port the exported function that calls it, with it.
- **`_group`** (22%): Hot inside `sqlparse/engine/`: port the exported function that calls it, with it.
- **`<lambda>`** (19%): Hot inside `sqlparse/`: port the exported function that calls it, with it.
- **`imt`** (19%): Mostly called by `<lambda>` (sqlparse/sql.py:274, 19535456 calls): port `<lambda>` together with `imt`, so one call into Rust does the whole loop.
- **`get_tokens`** (18%): Hot inside `sqlparse/`: port the exported function that calls it, with it.
- **`_group_matching`** (9%): Hot inside `sqlparse/engine/`: port the exported function that calls it, with it.
- **`match`** (9%): Hot inside `sqlparse/`: port the exported function that calls it, with it.
- **`_process`** (9%): Hot inside `sqlparse/filters/`: port the exported function that calls it, with it.
- **`_process_default`** (9%): Hot inside `sqlparse/filters/`: port the exported function that calls it, with it.
- **`group_typed_literal`** (6%): Hot and exported by `sqlparse/engine/`: port it (`rust-anything bridge upstream sqlparse/engine/`), then check `rust-anything bench`.
- **`_process_identifierlist`** (6%): Hot inside `sqlparse/filters/`: port the exported function that calls it, with it.
- **`_process_parenthesis`** (6%): Hot inside `sqlparse/filters/`: port the exported function that calls it, with it.

