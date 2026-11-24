# Limits and known differences

## Out of memory

sqlparse raises `MemoryError`. Rust cannot catch a failed allocation and stops the process instead. To keep this
from happening where an option makes the output grow:

- What grows with an option rather than with the input, the indent, is counted. Past 256 MiB of indent in one
  call, the call goes to sqlparse, which then raises `MemoryError` or answers as it does.
- An `indent_width` above 4096 goes to sqlparse at once.
- Texts of 2^29 characters or more go to sqlparse too.

An input that exhausts memory while being lexed or grouped can still stop the process where sqlparse would raise
`MemoryError`. sqlparse needs several times the memory for the same text, though.

## With a sqlparse other than 0.6.1

Every call goes to sqlparse through the package's function, one frame deeper than a direct call. So sqlparse's
recursion limit comes one nesting level sooner.

With 0.6.1, the calls the package leaves to sqlparse run sqlparse's code in the package's own frame, with no extra
frame.

## The `RecursionError` behind the recursion `SQLParseError`

It says `maximum recursion depth exceeded`. On CPython 3.10, sqlparse's also says where Python hit the limit
(`... in comparison`, `... in __instancecheck__`), which depends on the C call sqlparse was in. The `SQLParseError`
itself is the same.

## The front-door bindings

The bindings in `rust/sqlparse-front` (used for the recording) take typed arguments: `None` for a bool option is a
`TypeError` there, where sqlparse treats it as False. The package never sends such calls to the Rust.

Regenerating the bindings needs rust-anything with dotted exception kinds
([erkamyaman/rust-anything#207](https://github.com/erkamyaman/rust-anything/pull/207), not merged yet). The
bindings in this repository already raise sqlparse's own `SQLParseError`.

## Not tested

Windows on ARM, musl (Alpine), PyPy and free-threaded CPython. No wheels are built for them; pip builds from the
source distribution there, which needs a Rust toolchain.
