import { readdirSync, readFileSync } from "node:fs";

import { splitSqliteStatements } from "@alchemy.run/sql/SqlFile";
import { Effect } from "effect";
import { SqlClient } from "effect/sql";

const MIGRATIONS = new URL("../../migrations/", import.meta.url);

export const applyMigrations = Effect.gen(function* () {
  const sql = yield* SqlClient.SqlClient;
  for (const file of readdirSync(MIGRATIONS).sort()) {
    for (const statement of splitSqliteStatements(
      readFileSync(new URL(file, MIGRATIONS), "utf8"),
    )) {
      yield* sql.unsafe(statement);
    }
  }
});
