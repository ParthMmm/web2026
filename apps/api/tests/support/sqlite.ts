import { SqliteClient } from "@effect/sql-sqlite-node";
import { Layer } from "effect";

import { applyMigrations } from "./migrate.ts";

export const migratedSqlite = () =>
  Layer.effectDiscard(applyMigrations).pipe(
    Layer.provideMerge(SqliteClient.layer({ filename: ":memory:" }))
  );
