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
  page for its SQL panel with its own sqlparse filter stack. With `install()`:
  - the statements come grouped from the Rust, and the stack's `AlignedIndentFilter` runs there too;
  - only the toolbar's own keyword filter and serializer run in Python;
  - the panel formats the 400 corpus queries **5.8x faster** (4.70 s → 0.81 s), with identical HTML;
  - a page making 200 ORM queries loads **2.4x faster** through the toolbar (0.55 s → 0.23 s). The rest of that
    time is the queries themselves and the templates.

  Measured with [`bench/django_toolbar.py`](../bench/django_toolbar.py): Django 6.1, django-debug-toolbar 8.0,
  Python 3.13.
- **Migrations**: `RunSQL` scripts and `sqlmigrate` go through `sqlparse.split` and `sqlparse.format`.
- **Tests**: the test runner formats debug SQL with `sqlparse.format`.

## How it is checked

[`tools/check_django.py`](../tools/check_django.py) runs in CI on every platform and with the other sqlparse
versions:

- with sqlparse 0.6.1 the app turns the Rust on, and `RunSQL`'s split and the debug SQL equal sqlparse's;
- with another sqlparse nothing is replaced and the check reports `sqlparse_turbo.W001`.

`format_debug_sql` is checked on Django 6.0 and later only, which is where it exists.
