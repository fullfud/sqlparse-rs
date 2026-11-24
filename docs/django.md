# Django

Django and its tools call sqlparse themselves. Add the app to `settings.py` and all of them get the Rust, with
nothing else changed:

```python
INSTALLED_APPS = [
    ...,
    "sqlparse_turbo.django",
]
```

When Django starts, the app calls `sqlparse_turbo.install()`.

With a sqlparse other than 0.6.1 it replaces nothing, and `manage.py check` says so (`sqlparse_turbo.W001`). So a
project that pins another sqlparse does not quietly run at sqlparse speed.

Calling `sqlparse_turbo.install()` yourself, at the top of `settings.py`, does the same without the app.

## What gets faster

- **[django-debug-toolbar](https://github.com/django-commons/django-debug-toolbar)** formats every query of a
  page twice for its SQL panel, each with a sqlparse filter stack of its own filters: the formatted SQL
  (grouping, an `AlignedIndentFilter`, its `BoldKeywordFilter`, its `EscapedStringSerializer`) and the collapsed
  one (its `ElideSelectListsFilter` on the token stream, then the same two). With `install()` both run in Rust,
  its filters included, and the HTML comes straight out:
  - the panel formats the 400 corpus queries **62x faster** (4.89 s → 0.079 s), with identical HTML;
  - a page making 200 ORM queries loads **3.3x faster** through the toolbar (0.57 s → 0.17 s). Formatting SQL
    is now a sliver of that page: the rest is the queries themselves, the stack traces the toolbar records and
    the toolbar's templates.

  Measured with [`bench/django_toolbar.py`](../bench/django_toolbar.py): Django 6.1, django-debug-toolbar 8.0,
  Python 3.13.

  The toolbar's filters run in Rust only when they are exactly the code the Rust reproduces, which
  django-debug-toolbar 4.1 to 8.0 all have. The package reads the module's source once, checks the loaded
  functions were compiled from it, and on every call that nothing was replaced since (the classes, their
  methods and code, `escape`, the filter instances). Anything else, a toolbar version that changed them or a
  project that patches them, gets them run in Python, as before.
- **Migrations**: `RunSQL` scripts and `sqlmigrate` go through `sqlparse.split` and `sqlparse.format`.
- **Tests**: the test runner formats debug SQL with `sqlparse.format`.

## How it is checked

[`tools/check_toolbar.py`](../tools/check_toolbar.py) compares the toolbar's HTML with and without `install()`
in CI on every platform: the corpus, hand-picked cases (select lists without `FROM`, `UNION ALL`, `;` inside an
elided list, `ſelect`), 5,000 random token soups, statements too deep to group (the toolbar then runs its stack
again without grouping), callers near the recursion limit, a patched filter, `escape` or instance, and a stack
changed between two statements.

[`tools/check_django.py`](../tools/check_django.py) runs in CI on every platform and with the other sqlparse
versions:

- with sqlparse 0.6.1 the app turns the Rust on, and `RunSQL`'s split and the debug SQL equal sqlparse's;
- with another sqlparse nothing is replaced and the check reports `sqlparse_turbo.W001`.

`format_debug_sql` is checked on Django 6.0 and later only, which is where it exists.
