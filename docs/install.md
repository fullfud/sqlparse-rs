# Install

```sh
pip install sqlparse-turbo "sqlparse==0.6.1"   # sqlparse_turbo, the sqlformat-turbo command, and sqlparse 0.6.1
```

## Why sqlparse 0.6.1

The Rust is a port of sqlparse 0.6.1, sqlparse's current release, and runs only with it. Only sqlparse 0.6.1 gets
the Rust.

The package does not require that version: requiring it would stop a project pinned to another sqlparse from
installing it at all. So pip keeps whatever sqlparse a project already has. On another version everything works
as plain sqlparse, no faster, and the import warns:

```
RuntimeWarning: sqlparse_turbo runs in Rust only with sqlparse 0.6.1; sqlparse 0.5.3 is installed, so every call
goes to sqlparse at its own speed (pip install "sqlparse==0.6.1")
```

On such a version `install()` returns False and replaces nothing. In CI the package is checked with sqlparse
0.4.4, 0.5.3 and 0.6.0: every call is answered by that sqlparse and nothing reaches the Rust.

## Is the Rust on?

```python
import sqlparse_turbo
print(sqlparse_turbo.ACTIVE)   # True: calls run in Rust; False: every call goes to sqlparse
```

Even with `ACTIVE` True, a few inputs still go to sqlparse, so the answer stays sqlparse's: see
[When the Rust runs](fallback.md).

## Wheels

Wheels are built for:

- Linux x86_64 and aarch64 (manylinux2014);
- Windows x64;
- macOS, one universal2 wheel for arm64 and Intel.

Each is one abi3 wheel for every CPython from 3.10 on. Every platform and Python pair is tested in CI from that
wheel ([platforms](assets/platforms.svg)).

Elsewhere pip builds from the source distribution, which needs a Rust toolchain. That covers musl (Alpine),
Windows on ARM, PyPy and free-threaded CPython, none of which is tested (see [Limits](limits.md)).

## From a checkout

The package builds with [maturin](https://www.maturin.rs/):

```sh
pip install .
```

Or build wheels to copy elsewhere:

```sh
maturin build --release        # an abi3 wheel: one file for every CPython from 3.10 on
maturin build --release --zig --target x86_64-unknown-linux-gnu --compatibility manylinux2014 -i python3.11   # the Linux wheel, from Windows
```

The published wheels come from CI (`.github/workflows/ci.yml`), with maturin pinned to v1.16.0.
