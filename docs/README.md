# sqlparse-turbo documentation

The [README](../README.md) is the short version. Each page here covers one subject in full.

| Page | What is in it |
| --- | --- |
| [Install](install.md) | why sqlparse 0.6.1, the warning on other versions, wheels, building from source |
| [Usage](usage.md) | every function, `install()`, `FilterStack.run`, bytes and streams, `parsestream`, the CLI |
| [Django](django.md) | the app, check `W001`, django-debug-toolbar, migrations, tests |
| [When the Rust runs](fallback.md) | which calls go to sqlparse instead, patched sqlparse, Unicode versions |
| [Benchmarks](benchmarks.md) | every measurement, memory, against markocerkez/sqlparse-rs, how to measure yourself |
| [Verification](verification.md) | how the port is proven to give sqlparse's answers |
| [Internals](internals.md) | how the lexer matches Python's `re`, how the tree keeps sqlparse's quirks, what made it fast |
| [Limits](limits.md) | known differences and what is not tested |
| [Development](development.md) | repository layout, building, the checks, releases, the pictures |
| [Changelog](changelog.md) | what each version changed |

Reports the porting tool wrote along the way: [analyze.md](analyze.md) (the port plan for sqlparse) and
[profile-format.md](profile-format.md) (where sqlparse's `format()` spends its time).
