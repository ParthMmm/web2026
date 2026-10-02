//! Persistent local catalog: photo identities, source references, derivative
//! records, per-file import failures, and draft state.

use crate::{
    derivative::DerivativeSize,
    metadata::{CaptureTime, ExposureTime, PhotoMetadata},
};
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::{
    ffi::OsStr,
    fmt,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const SCHEMA_VERSION: i64 = 1;

/// Content identity: the BLAKE3 hash of the original's bytes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhotoId(Arc<str>);

impl PhotoId {
    pub fn from_hash(hash: blake3::Hash) -> Self {
        Self(hash.to_hex().as_str().into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PhotoId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Size and modification time recorded when a source was last hashed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceStamp {
    pub byte_size: u64,
    pub modified_ns: i64,
}

impl SourceStamp {
    pub fn read(path: &Path) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        let modified_ns = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX)
            });
        Ok(Self {
            byte_size: metadata.len(),
            modified_ns,
        })
    }
}

/// One photo as the library presents it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PhotoRecord {
    pub id: PhotoId,
    /// Preferred original: the first present source, by path.
    pub source: PathBuf,
    /// True when no known source path currently exists on disk.
    pub source_missing: bool,
    /// Number of paths known to contain these exact bytes.
    pub source_count: usize,
    /// Upright pixel dimensions of the original.
    pub width: u32,
    pub height: u32,
    pub byte_size: u64,
    pub imported_at: i64,
    pub metadata: PhotoMetadata,
}

impl PhotoRecord {
    pub fn file_name(&self) -> String {
        self.source
            .file_name()
            .unwrap_or(self.source.as_os_str())
            .to_string_lossy()
            .into_owned()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRecord {
    pub photo_id: PhotoId,
    pub stamp: SourceStamp,
    pub missing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DerivativeRecord {
    pub width: u32,
    pub height: u32,
    pub recipe: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportFailure {
    pub path: PathBuf,
    pub message: String,
    pub failed_at: i64,
}

/// Everything needed to insert a newly imported photo.
pub struct NewPhoto<'a> {
    pub id: &'a PhotoId,
    pub source: &'a Path,
    pub stamp: SourceStamp,
    pub width: u32,
    pub height: u32,
    pub metadata: &'a PhotoMetadata,
}

pub struct Catalog {
    connection: Connection,
}

impl Catalog {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)
            .with_context(|| format!("Couldn't open the library catalog at {}", path.display()))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        anyhow::ensure!(
            version <= SCHEMA_VERSION,
            "This library was created by a newer version of the app (schema {version})"
        );
        if version < 1 {
            connection.execute_batch(
                "BEGIN;
                CREATE TABLE photos (
                    id TEXT PRIMARY KEY,
                    byte_size INTEGER NOT NULL,
                    width INTEGER NOT NULL,
                    height INTEGER NOT NULL,
                    imported_at INTEGER NOT NULL,
                    status TEXT NOT NULL DEFAULT 'draft',
                    iso INTEGER,
                    exposure_numerator INTEGER,
                    exposure_denominator INTEGER,
                    f_number REAL,
                    focal_length_mm REAL,
                    camera_make TEXT,
                    camera_model TEXT,
                    lens TEXT,
                    captured_local TEXT,
                    captured_offset TEXT
                );
                CREATE TABLE sources (
                    path BLOB PRIMARY KEY,
                    photo_id TEXT NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                    byte_size INTEGER NOT NULL,
                    modified_ns INTEGER NOT NULL,
                    missing INTEGER NOT NULL DEFAULT 0,
                    added_at INTEGER NOT NULL
                );
                CREATE INDEX sources_by_photo ON sources(photo_id, missing, path);
                CREATE TABLE derivatives (
                    photo_id TEXT NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                    size TEXT NOT NULL,
                    width INTEGER NOT NULL,
                    height INTEGER NOT NULL,
                    recipe INTEGER NOT NULL,
                    PRIMARY KEY (photo_id, size)
                );
                CREATE TABLE import_failures (
                    path BLOB PRIMARY KEY,
                    message TEXT NOT NULL,
                    failed_at INTEGER NOT NULL
                );
                PRAGMA user_version = 1;
                COMMIT;",
            )?;
        }
        Ok(Self { connection })
    }

    /// All photos with at least one source, ordered by preferred source path.
    pub fn photos(&self) -> Result<Vec<PhotoRecord>> {
        let mut statement = self
            .connection
            .prepare(&format!("{PHOTO_SELECT} GROUP BY p.id"))?;
        let mut photos = statement
            .query_map([], photo_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        photos.sort_by(|a, b| a.source.cmp(&b.source));
        Ok(photos)
    }

    pub fn photo(&self, id: &PhotoId) -> Result<Option<PhotoRecord>> {
        Ok(self
            .connection
            .query_row(
                &format!("{PHOTO_SELECT} WHERE p.id = ?1 GROUP BY p.id"),
                [id.as_str()],
                photo_from_row,
            )
            .optional()?)
    }

    pub fn source(&self, path: &Path) -> Result<Option<SourceRecord>> {
        Ok(self
            .connection
            .query_row(
                "SELECT photo_id, byte_size, modified_ns, missing FROM sources WHERE path = ?1",
                [path_bytes(path)],
                |row| {
                    Ok(SourceRecord {
                        photo_id: PhotoId(row.get::<_, String>(0)?.into()),
                        stamp: SourceStamp {
                            byte_size: unsigned(row.get(1)?),
                            modified_ns: row.get(2)?,
                        },
                        missing: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn source_paths(&self) -> Result<Vec<(PathBuf, bool)>> {
        let mut statement = self
            .connection
            .prepare("SELECT path, missing FROM sources")?;
        let rows = statement
            .query_map([], |row| Ok((path_from_row(row, 0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn contains_photo(&self, id: &PhotoId) -> Result<bool> {
        Ok(self
            .connection
            .query_row("SELECT 1 FROM photos WHERE id = ?1", [id.as_str()], |_| {
                Ok(())
            })
            .optional()?
            .is_some())
    }

    pub fn insert_photo(&mut self, photo: NewPhoto<'_>) -> Result<()> {
        let now = unix_now();
        let metadata = photo.metadata;
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO photos (id, byte_size, width, height, imported_at, iso,
                exposure_numerator, exposure_denominator, f_number, focal_length_mm,
                camera_make, camera_model, lens, captured_local, captured_offset)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                photo.id.as_str(),
                signed(photo.stamp.byte_size),
                photo.width,
                photo.height,
                now,
                metadata.iso,
                metadata.exposure_time.map(ExposureTime::numerator),
                metadata.exposure_time.map(ExposureTime::denominator),
                metadata.f_number,
                metadata.focal_length_mm,
                metadata.camera_make,
                metadata.camera_model,
                metadata.lens,
                metadata.captured_at.as_ref().map(CaptureTime::local),
                metadata.captured_at.as_ref().and_then(CaptureTime::offset),
            ],
        )?;
        upsert_source(&transaction, photo.id, photo.source, photo.stamp, now)?;
        transaction.execute(
            "DELETE FROM import_failures WHERE path = ?1",
            [path_bytes(photo.source)],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Points `path` at `id`, replacing any previous assignment of that path.
    pub fn link_source(&mut self, id: &PhotoId, path: &Path, stamp: SourceStamp) -> Result<()> {
        let transaction = self.connection.transaction()?;
        upsert_source(&transaction, id, path, stamp, unix_now())?;
        transaction.execute(
            "DELETE FROM import_failures WHERE path = ?1",
            [path_bytes(path)],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn set_source_missing(&mut self, path: &Path, missing: bool) -> Result<()> {
        self.connection.execute(
            "UPDATE sources SET missing = ?2 WHERE path = ?1",
            params![path_bytes(path), missing],
        )?;
        Ok(())
    }

    /// Deletes photos that no longer have any source. Returns their IDs.
    pub fn remove_orphans(&mut self) -> Result<Vec<PhotoId>> {
        let mut statement = self.connection.prepare(
            "DELETE FROM photos WHERE id NOT IN (SELECT photo_id FROM sources) RETURNING id",
        )?;
        let ids = statement
            .query_map([], |row| Ok(PhotoId(row.get::<_, String>(0)?.into())))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }

    pub fn derivative(
        &self,
        id: &PhotoId,
        size: DerivativeSize,
    ) -> Result<Option<DerivativeRecord>> {
        Ok(self
            .connection
            .query_row(
                "SELECT width, height, recipe FROM derivatives WHERE photo_id = ?1 AND size = ?2",
                params![id.as_str(), size.key()],
                |row| {
                    Ok(DerivativeRecord {
                        width: row.get(0)?,
                        height: row.get(1)?,
                        recipe: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn record_derivative(
        &mut self,
        id: &PhotoId,
        size: DerivativeSize,
        record: DerivativeRecord,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO derivatives (photo_id, size, width, height, recipe)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (photo_id, size) DO UPDATE SET
                width = excluded.width, height = excluded.height, recipe = excluded.recipe",
            params![
                id.as_str(),
                size.key(),
                record.width,
                record.height,
                record.recipe
            ],
        )?;
        Ok(())
    }

    pub fn record_failure(&mut self, path: &Path, message: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO import_failures (path, message, failed_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (path) DO UPDATE SET message = excluded.message, failed_at = excluded.failed_at",
            params![path_bytes(path), message, unix_now()],
        )?;
        Ok(())
    }

    pub fn clear_failure(&mut self, path: &Path) -> Result<()> {
        self.connection.execute(
            "DELETE FROM import_failures WHERE path = ?1",
            [path_bytes(path)],
        )?;
        Ok(())
    }

    pub fn clear_failures(&mut self) -> Result<()> {
        self.connection.execute("DELETE FROM import_failures", [])?;
        Ok(())
    }

    pub fn failures(&self) -> Result<Vec<ImportFailure>> {
        let mut statement = self
            .connection
            .prepare("SELECT path, message, failed_at FROM import_failures ORDER BY path")?;
        let failures = statement
            .query_map([], |row| {
                Ok(ImportFailure {
                    path: path_from_row(row, 0)?,
                    message: row.get(1)?,
                    failed_at: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(failures)
    }
}

/// The preferred source is the first present path; `MIN` over a key that sorts
/// present paths before missing ones selects it in one pass.
const PHOTO_SELECT: &str = "
    SELECT p.id, p.byte_size, p.width, p.height, p.imported_at,
        p.iso, p.exposure_numerator, p.exposure_denominator, p.f_number, p.focal_length_mm,
        p.camera_make, p.camera_model, p.lens, p.captured_local, p.captured_offset,
        MIN(CAST(CAST(s.missing AS TEXT) || s.path AS BLOB)) AS preferred,
        COUNT(s.path) AS source_count
    FROM photos p JOIN sources s ON s.photo_id = p.id";

fn photo_from_row(row: &Row<'_>) -> rusqlite::Result<PhotoRecord> {
    let preferred: Vec<u8> = row.get("preferred")?;
    // The first byte is the text "0" for a present path and "1" for a missing one.
    let (missing, path) = preferred
        .split_first()
        .map_or((true, &[][..]), |(flag, path)| (*flag == b'1', path));
    let captured_local: Option<String> = row.get("captured_local")?;
    let captured_offset: Option<String> = row.get("captured_offset")?;
    let exposure = row
        .get::<_, Option<u32>>("exposure_numerator")?
        .zip(row.get::<_, Option<u32>>("exposure_denominator")?);
    Ok(PhotoRecord {
        id: PhotoId(row.get::<_, String>("id")?.into()),
        source: PathBuf::from(OsStr::from_bytes(path)),
        source_missing: missing,
        source_count: usize::try_from(row.get::<_, i64>("source_count")?).unwrap_or(0),
        width: row.get("width")?,
        height: row.get("height")?,
        byte_size: unsigned(row.get("byte_size")?),
        imported_at: row.get("imported_at")?,
        metadata: PhotoMetadata {
            iso: row.get("iso")?,
            exposure_time: exposure.and_then(|(n, d)| ExposureTime::new(n, d)),
            f_number: row.get("f_number")?,
            focal_length_mm: row.get("focal_length_mm")?,
            camera_make: row.get("camera_make")?,
            camera_model: row.get("camera_model")?,
            lens: row.get("lens")?,
            captured_at: captured_local.and_then(|local| CaptureTime::new(local, captured_offset)),
        },
    })
}

fn upsert_source(
    connection: &Connection,
    id: &PhotoId,
    path: &Path,
    stamp: SourceStamp,
    now: i64,
) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO sources (path, photo_id, byte_size, modified_ns, missing, added_at)
         VALUES (?1, ?2, ?3, ?4, 0, ?5)
         ON CONFLICT (path) DO UPDATE SET photo_id = excluded.photo_id,
            byte_size = excluded.byte_size, modified_ns = excluded.modified_ns, missing = 0",
        params![
            path_bytes(path),
            id.as_str(),
            signed(stamp.byte_size),
            stamp.modified_ns,
            now
        ],
    )
}

/// SQLite integers are signed 64-bit; file sizes never approach the limit.
fn signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn unsigned(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn path_bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_bytes()
}

fn path_from_row(row: &Row<'_>, ix: usize) -> rusqlite::Result<PathBuf> {
    Ok(PathBuf::from(OsStr::from_bytes(
        &row.get::<_, Vec<u8>>(ix)?,
    )))
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}
