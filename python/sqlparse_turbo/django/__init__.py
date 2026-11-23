"""A Django app that turns sqlparse-turbo on: add ``"sqlparse_turbo.django"`` to ``INSTALLED_APPS``.

When Django starts, the app calls :func:`sqlparse_turbo.install`, so Django's own sqlparse calls (``RunSQL`` and
``sqlmigrate`` in migrations, the test runner's debug SQL) and django-debug-toolbar's SQL panel go through the
Rust. When the installed sqlparse is not 0.6.1 nothing is replaced and ``manage.py check`` says so
(``sqlparse_turbo.W001``).
"""
