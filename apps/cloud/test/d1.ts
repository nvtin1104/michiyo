import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';

/** Minimal D1 stand-in on node:sqlite for unit tests (positional `?` parameters only). */
class Statement {
  constructor(
    private db: DatabaseSync,
    private sql: string,
    private params: unknown[] = [],
  ) {}
  bind(...params: unknown[]) {
    return new Statement(this.db, this.sql, params);
  }
  private rows(): Record<string, unknown>[] {
    return this.db.prepare(this.sql).all(...(this.params as never[])) as Record<string, unknown>[];
  }
  async first<T>(): Promise<T | null> {
    return (this.rows()[0] as T) ?? null;
  }
  async all<T>() {
    return { results: this.rows() as T[], success: true };
  }
  async run() {
    const info = this.db.prepare(this.sql).run(...(this.params as never[]));
    return { success: true, meta: { changes: Number(info.changes) } };
  }
}

export function createD1(): D1Database {
  const db = new DatabaseSync(':memory:');
  const dir = join(dirname(fileURLToPath(import.meta.url)), '../migrations');
  for (const file of readdirSync(dir).filter((f) => f.endsWith('.sql')).sort()) db.exec(readFileSync(join(dir, file), 'utf8'));
  return { prepare: (sql: string) => new Statement(db, sql) } as unknown as D1Database;
}
