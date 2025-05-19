// Type definitions for SQLite in Durable Objects
// These types help bridge the gap between Rust and JavaScript

interface DurableObjectStorage {
  sql: SqlStorage;
}

interface SqlStorage {
  exec(query: string, ...bindings: any[]): SqlStorageCursor;
  prepare(query: string): SqlPreparedStatement;
  dump(): ArrayBuffer;
  txn?: SqlTransaction;
}

interface SqlStorageCursor {
  readonly rowsRead: number;
  readonly rowsWritten: number;
  readonly durationMs: number;
  
  next(): any | undefined;
  toArray(): any[];
  forEach(callback: (row: any, index: number) => void): void;
}

interface SqlPreparedStatement {
  bind(...params: any[]): SqlStorageCursor;
  first(...params: any[]): any | null;
  all(...params: any[]): SqlResults;
  run(...params: any[]): SqlMeta;
}

interface SqlResults {
  readonly results: any[];
  readonly success: boolean;
  readonly meta: SqlMeta;
}

interface SqlMeta {
  readonly duration: number;
  readonly size_after?: number;
  readonly size_before?: number;
  readonly rows_written: number;
  readonly rows_read: number;
}

interface SqlTransaction {
  rollback(): void;
}