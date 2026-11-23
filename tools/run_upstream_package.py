"""sqlparse's own test suite with the package installed (sqlparse.format/split go through sqlparse_turbo):
python tools/run_upstream_package.py [pytest args]

Prints how many calls the Rust took, so a pass is known to have run the port. The calls are counted with a
profile hook (C calls add no Python frame), so the stack depth the Rust sees is the one it sees in use."""
import os
import sys

import pytest

import sqlparse_turbo

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


class Counted:
    def __init__(self):
        self.counts = {}

    def pytest_configure(self, config):
        assert sqlparse_turbo.install()
        native = {sqlparse_turbo._native.format: 'format', sqlparse_turbo._native.split: 'split',
                  sqlparse_turbo._native.lex: 'lex', sqlparse_turbo._native.parse: 'parse'}
        counts = self.counts = {'format': 0, 'split': 0, 'lex': 0, 'parse': 0}

        def hook(frame, event, arg):
            if event == 'c_call':
                name = native.get(arg)
                if name:
                    counts[name] += 1

        sys.setprofile(hook)

    def pytest_sessionfinish(self, session, exitstatus):
        sys.setprofile(None)
        import sqlparse
        installed = sqlparse.format is sqlparse_turbo.format and sqlparse.split is sqlparse_turbo.split
        print(f'\n[sqlparse_turbo] installed: {installed}; calls run in Rust: format {self.counts["format"]}, '
              f'split {self.counts["split"]}, parse {self.counts["parse"]}, lexer {self.counts["lex"]}')


os.chdir(os.path.join(ROOT, 'upstream'))
sys.exit(pytest.main(['-q', '-p', 'no:cacheprovider', *sys.argv[1:]], plugins=[Counted()]))
