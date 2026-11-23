"""The Django app: "sqlparse_turbo.django" in INSTALLED_APPS turns the Rust on when Django starts, Django's own
sqlparse calls give sqlparse's results through it, and `manage.py check` warns (sqlparse_turbo.W001) when the
installed sqlparse is not the ported version and nothing was replaced.

    pip install django && python tools/check_django.py
"""

import sys
import warnings

import django
from django.conf import settings

warnings.simplefilter('ignore')  # the import warning with another sqlparse is checked in check_other_sqlparse.py
import sqlparse  # noqa: E402
import sqlparse_turbo  # noqa: E402

original_format, original_split = sqlparse.format, sqlparse.split
settings.configure(
    INSTALLED_APPS=['sqlparse_turbo.django'],
    DATABASES={'default': {'ENGINE': 'django.db.backends.sqlite3', 'NAME': ':memory:'}},
)
django.setup()

from django.core.checks import run_checks  # noqa: E402
from django.db import connection  # noqa: E402

bad = []
ids = [m.id for m in run_checks()]
if sqlparse_turbo.ACTIVE:
    if sqlparse.format is not sqlparse_turbo.format or sqlparse.split is not sqlparse_turbo.split:
        bad.append('the app did not install the Rust')
    if 'sqlparse_turbo.W001' in ids:
        bad.append('W001 with the ported sqlparse')
else:
    if sqlparse.format is not original_format or sqlparse.split is not original_split:
        bad.append('the app replaced sqlparse although its version is not the ported one')
    if 'sqlparse_turbo.W001' not in ids:
        bad.append(f'no W001 with sqlparse {sqlparse.__version__}: {ids}')

# what RunSQL and the test runner's debug output call, against sqlparse's own functions
script = ("create table t (id integer primary key, name text); -- the table\n"
          "insert into t values (1, 'a;b'); select case when id > 1 then 'x' else 'y' end from t where name = 'a';")
sql = "select a, b, count(*) from t join u on t.id = u.t_id where x in (1, 2) group by a, b order by 3 desc"
if connection.ops.prepare_sql_script(script) != [
        s for s in original_split(sqlparse_turbo._original_format(script, strip_comments=True)) if s]:
    bad.append('prepare_sql_script differs from sqlparse')
if hasattr(connection.ops, 'format_debug_sql') and (  # Django 6.0+
        connection.ops.format_debug_sql(sql) != sqlparse_turbo._original_format(sql, reindent=True, keyword_case='upper')):
    bad.append('format_debug_sql differs from sqlparse')
with connection.cursor() as cursor:  # the statements split by the Rust run
    for statement in connection.ops.prepare_sql_script(script):
        cursor.execute(statement)

print(f'Django {django.__version__}, sqlparse {sqlparse.__version__}, active {sqlparse_turbo.ACTIVE}, checks {ids}')
for b in bad:
    print('FAIL', b)
sys.exit(1 if bad else 0)
