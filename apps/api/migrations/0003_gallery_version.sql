-- One row whose version rises with every change to what the gallery manifest
-- lists. The trigger runs inside the UPDATE that changes a photo, so no
-- transaction is needed: a manifest writer that reads the same version before
-- and after its put knows it wrote the current list. To undo, drop the trigger
-- and the table.
CREATE TABLE gallery (
  id      INTEGER PRIMARY KEY CHECK (id = 1),
  version INTEGER NOT NULL
);
INSERT INTO gallery (id, version) VALUES (1, 0);
CREATE TRIGGER gallery_version AFTER UPDATE OF publication, details ON photo
WHEN OLD.publication IS NOT NEW.publication OR OLD.details IS NOT NEW.details
BEGIN
  UPDATE gallery SET version = version + 1 WHERE id = 1;
END;
