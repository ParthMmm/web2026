-- publication NULL means private: Uploading while finalized_at is NULL, else Draft.
-- 'Published' and 'Unpublishing' mean the public bucket may hold copies. Each
-- transition is one UPDATE, and the checks refuse any other combination.
ALTER TABLE photo ADD COLUMN publication TEXT
  CHECK (
    publication IS NULL
    OR (publication IN ('Published', 'Unpublishing') AND finalized_at IS NOT NULL)
  );
ALTER TABLE photo ADD COLUMN published_at INTEGER
  CHECK ((published_at IS NULL) = (publication IS NULL));
ALTER TABLE photo ADD COLUMN details TEXT
  CHECK ((details IS NULL) = (publication IS NULL));
