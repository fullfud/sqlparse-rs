"""Runs sqlparse with the Rust lexer: install() replaces sqlparse.lexer.tokenize, which is what sqlparse's own
pipeline calls (engine/filter_stack.py), so parse, format and split all lex in Rust.

The Rust lexer is the default lexer and nothing else. Anything outside what it was proven on goes to the
original, unchanged: bytes or file input, an `encoding`, a default lexer whose keywords or SQL_REGEX were
changed (Lexer.add_keywords, set_SQL_REGEX, clear, default_initialization), and text the Rust side cannot take
(a str with lone surrogates).
"""

import glob
import importlib.util
import os

import sqlparse
from sqlparse import lexer as _lexer
from sqlparse import tokens as _tokens

_HERE = os.path.dirname(os.path.abspath(__file__))
_original = _lexer.tokenize


def _load_native():
    pyd = glob.glob(os.path.join(_HERE, '..', 'rust', 'sqlparse-front', 'python', 'sqlparse_front', '_native*'))
    spec = importlib.util.spec_from_file_location('_native', pyd[0])
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


_native = _load_native()
_types = {}


def _ttype(name):
    t = _types.get(name)
    if t is None:
        t = _tokens.Token
        for part in name.split('.')[1:]:
            t = getattr(t, part)
        _types[name] = t
    return t


def _regex_content(rx_list):
    return [(m.__self__.pattern, m.__self__.flags, tt) for m, tt in rx_list]


# The default lexer as installed: the keyword dicts themselves (held, so compared by identity, never by a
# reusable id()), their sizes, and SQL_REGEX by content. A list object seen to hold the default rules is
# remembered so the common call compares one reference.
_default_keywords = None
_default_sizes = None
_default_rx = None
_rx_seen = None


def _is_default(lx):
    global _rx_seen
    kws = lx._keywords
    if len(kws) != len(_default_keywords):
        return False
    for d, default, size in zip(kws, _default_keywords, _default_sizes):
        if d is not default or len(d) != size:
            return False
    rx = lx._SQL_REGEX
    if rx is _rx_seen:
        return True
    if _regex_content(rx) != _default_rx:
        return False
    _rx_seen = rx
    return True


def tokenize(sql, encoding=None):
    lx = _lexer.Lexer.get_default_instance()
    if type(sql) is not str or encoding is not None or not _is_default(lx):
        return _original(sql, encoding)
    try:
        pairs = _native.tokenize(sql)
    except UnicodeEncodeError:
        return _original(sql, encoding)
    return ((_ttype(t), v) for t, v in pairs)  # a generator, as sqlparse's own tokenize returns


def install():
    global _default_keywords, _default_sizes, _default_rx, _rx_seen
    ref = _lexer.Lexer()  # a fresh default lexer: what the Rust tables were generated from
    ref.default_initialization()  # (the shared instance is left as the caller configured it)
    _default_keywords = list(ref._keywords)
    _default_sizes = [len(d) for d in _default_keywords]
    _default_rx = _regex_content(ref._SQL_REGEX)
    _rx_seen = None
    _lexer.tokenize = tokenize


def uninstall():
    _lexer.tokenize = _original


__all__ = ['install', 'uninstall', 'tokenize', 'sqlparse']
