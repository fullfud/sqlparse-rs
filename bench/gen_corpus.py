"""Writes the benchmark corpus: SQL files of the kinds sqlparse meets in practice, from a fixed seed.

    python bench/gen_corpus.py [out dir]      (default bench/corpus)

Kinds: reporting SELECTs (joins, CASE, subqueries, CTEs, window functions), schema scripts (CREATE TABLE,
indexes, ALTERs), batched INSERTs, PL/pgSQL functions, and migration-style scripts mixing all of them with
comments. Same seed, same bytes: the corpus is not committed, only this generator.
"""

import os
import random
import sys

R = random.Random(20261122)

TABLES = ['users', 'orders', 'order_items', 'products', 'categories', 'payments', 'shipments', 'stations',
          'trains', 'routes', 'tickets', 'audit_log', 'sessions', 'inventory', 'suppliers', 'warehouses']
COLS = ['id', 'name', 'created_at', 'updated_at', 'status', 'amount', 'price', 'quantity', 'user_id', 'order_id',
        'product_id', 'category_id', 'email', 'title', 'description', 'total', 'currency', 'country', 'city',
        'deleted', 'score', 'rank', 'parent_id', 'code', 'weight', 'note']
TYPES = ['INTEGER', 'BIGINT', 'VARCHAR(255)', 'TEXT', 'NUMERIC(12, 2)', 'TIMESTAMP WITH TIME ZONE', 'BOOLEAN',
         'DATE', 'JSONB', 'UUID']
FUNCS = ['COUNT', 'SUM', 'AVG', 'MIN', 'MAX', 'COALESCE', 'LOWER', 'UPPER', 'TRIM', 'ROUND', 'DATE_TRUNC']


def ident(t=None):
    c = R.choice(COLS)
    return f'{t}.{c}' if t else c


def literal():
    k = R.random()
    if k < 0.3:
        return str(R.randint(0, 100000))
    if k < 0.5:
        return f'{R.uniform(0, 1000):.2f}'
    if k < 0.8:
        word = R.choice(['active', 'pending', "O'Brien", 'RU', 'Москва', 'closed', '%test%', 'a_b'])
        return "'" + word.replace("'", "''") + "'"
    return R.choice(['NULL', 'TRUE', 'FALSE', 'CURRENT_TIMESTAMP', "INTERVAL '7 days'"])


def expr(aliases, depth=0):
    a = R.choice(aliases)
    k = R.random()
    if depth > 2 or k < 0.35:
        return ident(a)
    if k < 0.55:
        return f'{R.choice(FUNCS)}({ident(a)})'
    if k < 0.7:
        return f'({expr(aliases, depth + 1)} {R.choice("+-*/")} {expr(aliases, depth + 1)})'
    if k < 0.85:
        whens = ' '.join(f'WHEN {cond(aliases, depth + 1)} THEN {literal()}' for _ in range(R.randint(1, 4)))
        return f'CASE {whens} ELSE {literal()} END'
    return f'CAST({ident(a)} AS {R.choice(TYPES)})'


def cond(aliases, depth=0):
    a = R.choice(aliases)
    k = R.random()
    if k < 0.4:
        return f'{ident(a)} {R.choice(["=", "<>", "<", ">=", "LIKE"])} {literal()}'
    if k < 0.55:
        return f'{ident(a)} IN ({", ".join(literal() for _ in range(R.randint(2, 6)))})'
    if k < 0.65:
        return f'{ident(a)} IS {R.choice(["", "NOT "])}NULL'
    if k < 0.75:
        return f'{ident(a)} BETWEEN {literal()} AND {literal()}'
    if k < 0.85 and depth < 2:
        return f'EXISTS (SELECT 1 FROM {R.choice(TABLES)} x WHERE x.{R.choice(COLS)} = {ident(a)})'
    if depth < 2:
        return f'({cond(aliases, depth + 1)} {R.choice(["AND", "OR"])} {cond(aliases, depth + 1)})'
    return f'{ident(a)} = {literal()}'


def select(depth=0):
    n = R.randint(1, 5)
    tables = R.sample(TABLES, n)
    aliases = [f't{i}' for i in range(n)]
    cols = ', '.join(
        f'{expr(aliases)} AS c{i}' if R.random() < 0.4 else expr(aliases) for i in range(R.randint(3, 14)))
    if R.random() < 0.15:
        cols += f', ROW_NUMBER() OVER (PARTITION BY {ident(aliases[0])} ORDER BY {ident(aliases[-1])} DESC) AS rn'
    sql = f'SELECT {"DISTINCT " if R.random() < 0.1 else ""}{cols}\nFROM {tables[0]} t0'
    for i in range(1, n):
        sql += (f'\n{R.choice(["JOIN", "LEFT JOIN", "INNER JOIN", "LEFT OUTER JOIN"])} {tables[i]} t{i} '
                f'ON t{i}.{R.choice(COLS)} = t{i - 1}.{R.choice(COLS)}')
    if depth < 1 and R.random() < 0.25:
        sql += f'\nJOIN ({select(depth + 1)}) sub ON sub.c0 = t0.id'
    sql += f'\nWHERE {" AND ".join(cond(aliases) for _ in range(R.randint(1, 5)))}'
    if R.random() < 0.4:
        sql += f'\nGROUP BY {", ".join(ident(R.choice(aliases)) for _ in range(R.randint(1, 3)))}'
        if R.random() < 0.4:
            sql += f'\nHAVING COUNT(*) > {R.randint(1, 50)}'
    if R.random() < 0.6:
        sql += f'\nORDER BY {ident(R.choice(aliases))} {R.choice(["ASC", "DESC"])}'
    if R.random() < 0.4:
        sql += f'\nLIMIT {R.randint(1, 500)} OFFSET {R.randint(0, 5000)}'
    return sql


def with_cte():
    ctes = ',\n'.join(f'cte{i} AS (\n{select(1)}\n)' for i in range(R.randint(1, 3)))
    return f'WITH {ctes}\n{select()}'


def create_table():
    t = R.choice(TABLES) + f'_{R.randint(1, 999)}'
    cols = [f'    {c} {R.choice(TYPES)}{R.choice(["", " NOT NULL", " DEFAULT 0", " UNIQUE"])}'
            for c in R.sample(COLS, R.randint(4, 12))]
    body = ',\n'.join(['    id BIGSERIAL PRIMARY KEY'] + cols)
    sql = f'CREATE TABLE IF NOT EXISTS {t} (\n{body}\n);\n'
    for _ in range(R.randint(0, 3)):
        sql += f'CREATE INDEX idx_{t}_{R.choice(COLS)} ON {t} ({R.choice(COLS)});\n'
    if R.random() < 0.5:
        sql += f'ALTER TABLE {t} ADD COLUMN {R.choice(COLS)}_x {R.choice(TYPES)};\n'
    return sql


def insert_batch():
    t = R.choice(TABLES)
    cols = R.sample(COLS, R.randint(3, 8))
    rows = ',\n'.join('(' + ', '.join(literal() for _ in cols) + ')' for _ in range(R.randint(5, 60)))
    return f'INSERT INTO {t} ({", ".join(cols)}) VALUES\n{rows};\n'


def plpgsql():
    name = f'fn_{R.choice(TABLES)}_{R.randint(1, 99)}'
    body = '\n'.join(f'    IF {cond(["t0"])} THEN\n        UPDATE {R.choice(TABLES)} SET {R.choice(COLS)} = {literal()} '
                     f'WHERE id = p_id;\n    END IF;' for _ in range(R.randint(1, 5)))
    return (f'CREATE OR REPLACE FUNCTION {name}(p_id BIGINT) RETURNS VOID AS $$\nBEGIN\n{body}\n'
            f'    RETURN;\nEND;\n$$ LANGUAGE plpgsql;\n')


def comment():
    return R.choice(['-- migration step\n', '/* generated, do not edit */\n', '-- TODO: check indexes\n', ''])


def script(n):
    parts = []
    for _ in range(n):
        k = R.random()
        if k < 0.35:
            parts.append(select() + ';\n')
        elif k < 0.5:
            parts.append(with_cte() + ';\n')
        elif k < 0.65:
            parts.append(create_table())
        elif k < 0.85:
            parts.append(insert_batch())
        else:
            parts.append(plpgsql())
        parts.append(comment())
    return '\n'.join(parts)


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), 'corpus')
    os.makedirs(out, exist_ok=True)
    total = 0
    # many small files (one query each, the common call) and a few large scripts
    for i in range(400):
        sql = select() + ';\n' if R.random() < 0.7 else with_cte() + ';\n'
        total += write(out, f'q{i:03d}.sql', sql)
    for i in range(40):
        total += write(out, f'script{i:02d}.sql', script(R.randint(10, 60)))
    print(f'wrote 440 files, {total / 1e6:.1f} MB, to {out}')


def write(out, name, sql):
    data = sql.encode('utf-8')
    with open(os.path.join(out, name), 'wb') as f:
        f.write(data)
    return len(data)


if __name__ == '__main__':
    main()
