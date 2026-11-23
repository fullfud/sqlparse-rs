import sqlparse
from django.apps import AppConfig
from django.core import checks

import sqlparse_turbo


def check_active(app_configs=None, **kwargs):
    """``sqlparse_turbo.W001`` when the Rust is not in use: the installed sqlparse is not the ported version."""
    if sqlparse_turbo.ACTIVE:
        return []
    return [checks.Warning(
        f'sqlparse-turbo is off: sqlparse {sqlparse.__version__} is installed and the Rust port is of sqlparse '
        f'{sqlparse_turbo.PORTED_VERSION}, so every call goes to sqlparse itself, at sqlparse speed.',
        hint=f'pip install "sqlparse=={sqlparse_turbo.PORTED_VERSION}"',
        id='sqlparse_turbo.W001',
    )]


class SqlparseTurboConfig(AppConfig):
    name = 'sqlparse_turbo.django'
    label = 'sqlparse_turbo'
    verbose_name = 'sqlparse-turbo'

    def ready(self):
        sqlparse_turbo.install()
        checks.register(check_active)
