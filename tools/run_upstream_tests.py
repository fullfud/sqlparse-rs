"""sqlparse's own test suite with the Rust lexer installed: python tools/run_upstream_tests.py [pytest args]"""
import os, sys
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, 'front'))
import pytest
import rs_lexer

class Installed:
    def pytest_configure(self, config):
        rs_lexer.install()

    def pytest_sessionfinish(self, session, exitstatus):
        n = rs_lexer._native
        print(f'\n[rs_lexer] tokenize replaced: {rs_lexer._lexer.tokenize is rs_lexer.tokenize}')

os.chdir(os.path.join(ROOT, 'upstream'))
sys.exit(pytest.main(['-q', '-p', 'no:cacheprovider', *sys.argv[1:]], plugins=[Installed()]))
