"""django-debug-toolbar with and without sqlparse_turbo.install(): the SQL panel's own formatting of the 400
corpus queries (reformat_sql, the HTML compared equal), and a whole page making N distinct ORM queries requested
through the toolbar.

    pip install django django-debug-toolbar && python -X utf8 bench/django_toolbar.py [N]
"""
import sys
import time

import django
from django.conf import settings

N = int(sys.argv[1]) if len(sys.argv) > 1 else 200

settings.configure(
    DEBUG=True,
    SECRET_KEY='x',
    ALLOWED_HOSTS=['*'],
    ROOT_URLCONF=__name__,
    INTERNAL_IPS=['127.0.0.1'],
    INSTALLED_APPS=['django.contrib.auth', 'django.contrib.contenttypes', 'django.contrib.staticfiles',
                    'debug_toolbar'],
    MIDDLEWARE=['debug_toolbar.middleware.DebugToolbarMiddleware'],
    DATABASES={'default': {'ENGINE': 'django.db.backends.sqlite3', 'NAME': ':memory:'}},
    STATIC_URL='/static/',
    TEMPLATES=[{'BACKEND': 'django.template.backends.django.DjangoTemplates', 'APP_DIRS': True}],
    DEBUG_TOOLBAR_CONFIG={'SHOW_TOOLBAR_CALLBACK': lambda r: True, 'RENDER_PANELS': True},
)
django.setup()

from django.contrib.auth.models import Group, Permission, User  # noqa: E402
from django.core.management import call_command  # noqa: E402
from django.db.models import Count, Q  # noqa: E402
from django.http import HttpResponse  # noqa: E402
from django.test import Client  # noqa: E402
from django.urls import include, path  # noqa: E402
from debug_toolbar.panels.sql import utils  # noqa: E402
from debug_toolbar.toolbar import debug_toolbar_urls  # noqa: E402

import sqlparse_turbo  # noqa: E402


def page(request):
    rows = 0
    for i in range(N):
        kind = i % 4
        if kind == 0:
            qs = User.objects.filter(Q(username__startswith=f'u{i}') | Q(email__icontains=f'{i}@')).select_related()
        elif kind == 1:
            qs = Group.objects.filter(name__in=[f'g{i}', f'h{i}']).annotate(n=Count('user')).order_by('-n', 'name')
        elif kind == 2:
            qs = Permission.objects.filter(content_type__app_label='auth', codename__contains=f'{i}') \
                .select_related('content_type').order_by('codename')[:10]
        else:
            qs = User.objects.filter(groups__name=f'g{i}', is_active=True).exclude(last_login=None) \
                .values('id', 'username').distinct()
        rows += len(list(qs))
    return HttpResponse(f'<html><body>{rows} rows</body></html>')


urlpatterns = [path('', page)] + debug_toolbar_urls()
call_command('migrate', verbosity=0)
client = Client()


def timed():
    best = None
    for _ in range(3):
        utils.parse_sql.cache_clear()
        t = time.perf_counter()
        response = client.get('/')
        d = time.perf_counter() - t
        assert response.status_code == 200 and b'djDebug' in response.content
        best = d if best is None else min(best, d)
    return best


plain_t = timed()
assert sqlparse_turbo.install()
turbo_t = timed()
sqlparse_turbo.uninstall()
print(f'page with {N} queries through django-debug-toolbar: sqlparse {plain_t:.3f} s, '
      f'sqlparse_turbo.install() {turbo_t:.3f} s ({plain_t / turbo_t:.2f}x)')


def panel_formatting():
    import glob
    import os
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    files = [open(p, encoding='utf-8').read() for p in sorted(glob.glob(os.path.join(root, 'bench', 'corpus', 'q*.sql')))]

    def run():
        best, out = None, None
        for _ in range(3):
            utils.parse_sql.cache_clear()
            t = time.perf_counter()
            out = [utils.reformat_sql(s, with_toggle=True) for s in files]
            d = time.perf_counter() - t
            best = d if best is None else min(best, d)
        return best, out

    a, out_a = run()
    assert sqlparse_turbo.install()
    b, out_b = run()
    sqlparse_turbo.uninstall()
    assert out_a == out_b, 'the panel HTML differs'
    print(f'SQL panel formatting, {len(files)} queries: sqlparse {a:.3f} s, sqlparse_turbo.install() {b:.3f} s '
          f'({a / b:.2f}x), same HTML')


panel_formatting()
