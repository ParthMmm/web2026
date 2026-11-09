-- finalized_at NULL means uploading; set means verified draft. No second state column to keep in step.
CREATE TABLE photo (
  photo_id     TEXT PRIMARY KEY,
  manifest     TEXT NOT NULL,
  created_at   INTEGER NOT NULL,
  finalized_at INTEGER
);
