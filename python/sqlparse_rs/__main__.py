"""`sqlformat-rs` / `python -m sqlparse_rs`: sqlparse's own `sqlformat` command, with format() in Rust.

Same arguments, output and exit codes as `sqlformat` (sqlparse.cli), which looks sqlparse.format up on the module
when it runs, so install() is all it takes.
"""

import sys

from sqlparse import cli

from . import install


def main(args=None):
    install()
    return cli.main(args)


if __name__ == '__main__':
    sys.exit(main())
