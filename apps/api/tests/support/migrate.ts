import { readdirSync, readFileSync } from "node:fs";

import { splitSqliteStatements } from "@alchemy.run/sql/SqlFile";
import { Effect } from "effect";
import { SqlClient } from "effect/sql";

const MIGRATIONS = new URL("../../migrations/", import.meta.url);

export const applyMigrations = Effect.gen(function* applyMigrations() {
  const sql = yield* SqlClient.SqlClient;
  for (const file of readdirSync(MIGRATIONS).toSorted()) {
    for (const statement of splitSqliteStatements(
      readFileSync(new URL(file, MIGRATIONS), "utf-8")
    )) {
      yield* sql.unsafe(statement);
    }
  }
});
