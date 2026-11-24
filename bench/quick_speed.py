"""sqlparse against sqlparse-turbo on the same SQL, on your machine, in a few seconds.

    pip install "sqlparse==0.6.1" sqlparse-turbo
    python quick_speed.py

Put your own SQL in QUERY to try it on that.
"""
import time

import sqlparse
import sqlparse_turbo as turbo

QUERY = """
select u.id, u.name, count(o.id) as orders, sum(o.total) as spent,
       case when sum(o.total) > 1000 then 'gold' when sum(o.total) > 100 then 'silver' else 'basic' end as tier
from users u
left join orders o on o.user_id = u.id and o.created_at >= '2026-01-01'
where u.active = 1 and u.country in ('RU', 'KZ', 'BY')
  and exists (select 1 from sessions s where s.user_id = u.id and s.started_at > now() - interval '30 days')
group by u.id, u.name
having count(o.id) > 2
order by spent desc
limit 100;
insert into audit (user_id, action, payload) values (42, 'login', '{"ip": "10.0.0.1"}');
update users set last_seen = now() where id = 42;
"""
SQL = QUERY * 30  # 90 statements, about 18 KB


def best(fn, runs=3):
    """The best of a few runs, in seconds."""
    times = []
    for _ in range(runs):
        start = time.perf_counter()
        fn()
        times.append(time.perf_counter() - start)
    return min(times)


print(f"sqlparse {sqlparse.__version__}, Rust on: {turbo.ACTIVE}")
if not turbo.ACTIVE:
    print('No speed-up: sqlparse-turbo runs in Rust only with sqlparse 0.6.1 (pip install "sqlparse==0.6.1")')
print(f"SQL: {len(SQL) // 1024} KB, {len(sqlparse.split(SQL))} statements\n")

tests = {
    "format(reindent, upper)": lambda lib: lib.format(SQL, reindent=True, keyword_case="upper"),
    "split": lambda lib: lib.split(SQL),
    "parse": lambda lib: [str(s) for s in lib.parse(SQL)],
}
print(f"{'':26}{'sqlparse':>12}{'turbo':>12}{'faster':>10}")
for name, call in tests.items():
    same = call(sqlparse) == call(turbo)
    slow = best(lambda: call(sqlparse))
    fast = best(lambda: call(turbo))
    note = "" if same else "   DIFFERENT RESULT!"
    print(f"{name:26}{slow * 1000:>9.0f} ms{fast * 1000:>9.1f} ms{slow / fast:>9.0f}x{note}")
