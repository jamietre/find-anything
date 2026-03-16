use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

pub struct LinkRow {
    pub source: String,
    /// Outer file path (no `::` suffix).
    pub path: String,
    /// Inner archive member path if this is a composite path.
    pub archive_path: Option<String>,
    pub kind: String,
    pub mtime: i64,
    pub expires_at: i64,
}

pub enum ResolveResult {
    Found(LinkRow),
    Expired,
    NotFound,
}

pub fn open_links_db(data_dir: &Path) -> Result<Connection> {
    let db_path = data_dir.join("links.db");
    let conn = Connection::open(&db_path)
        .with_context(|| format!("opening {}", db_path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS links (
            code         TEXT PRIMARY KEY,
            source       TEXT NOT NULL,
            path         TEXT NOT NULL,
            archive_path TEXT,
            kind         TEXT NOT NULL DEFAULT 'text',
            mtime        INTEGER NOT NULL DEFAULT 0,
            created_at   INTEGER NOT NULL,
            expires_at   INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS links_expires ON links(expires_at);",
    )
    .context("creating links table")?;
    Ok(conn)
}

pub struct CreateLinkArgs<'a> {
    pub code: &'a str,
    pub source: &'a str,
    pub path: &'a str,
    pub archive_path: Option<&'a str>,
    pub kind: &'a str,
    pub mtime: i64,
    pub created_at: i64,
    pub expires_at: i64,
}

pub fn create_link(conn: &Connection, a: &CreateLinkArgs<'_>) -> Result<()> {
    conn.execute(
        "INSERT INTO links (code, source, path, archive_path, kind, mtime, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![a.code, a.source, a.path, a.archive_path, a.kind, a.mtime, a.created_at, a.expires_at],
    )
    .context("inserting link")?;
    Ok(())
}

pub fn resolve_link(conn: &Connection, code: &str) -> Result<ResolveResult> {
    let now = unix_now();
    let result = conn
        .query_row(
            "SELECT source, path, archive_path, kind, mtime, expires_at
             FROM links WHERE code = ?1",
            params![code],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()?;

    match result {
        None => Ok(ResolveResult::NotFound),
        Some((source, path, archive_path, kind, mtime, expires_at)) => {
            if expires_at < now {
                Ok(ResolveResult::Expired)
            } else {
                Ok(ResolveResult::Found(LinkRow {
                    source,
                    path,
                    archive_path,
                    kind,
                    mtime,
                    expires_at,
                }))
            }
        }
    }
}

pub fn sweep_expired(conn: &Connection) -> Result<usize> {
    let now = unix_now();
    let deleted = conn
        .execute("DELETE FROM links WHERE expires_at < ?1", params![now])
        .context("sweeping expired links")?;
    Ok(deleted)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_temp_db() -> (TempDir, Connection) {
        let dir = TempDir::new().unwrap();
        let conn = open_links_db(dir.path()).unwrap();
        (dir, conn)
    }

    fn make_args<'a>(code: &'a str, now: i64, ttl: i64) -> CreateLinkArgs<'a> {
        CreateLinkArgs {
            code,
            source: "src",
            path: "foo/bar.jpg",
            archive_path: None,
            kind: "image",
            mtime: 1_700_000_000,
            created_at: now,
            expires_at: now + ttl,
        }
    }

    #[test]
    fn create_and_resolve() {
        let (_dir, conn) = open_temp_db();
        let now = unix_now();
        create_link(&conn, &make_args("ABCDEF", now, 3600)).unwrap();
        match resolve_link(&conn, "ABCDEF").unwrap() {
            ResolveResult::Found(row) => {
                assert_eq!(row.source, "src");
                assert_eq!(row.path, "foo/bar.jpg");
                assert_eq!(row.kind, "image");
            }
            other => panic!("expected Found, got {:?}", std::mem::discriminant(&other)),
        }
    }

    #[test]
    fn not_found() {
        let (_dir, conn) = open_temp_db();
        assert!(matches!(resolve_link(&conn, "XXXXXX").unwrap(), ResolveResult::NotFound));
    }

    #[test]
    fn expired_code() {
        let (_dir, conn) = open_temp_db();
        let past = unix_now() - 1000;
        let args = CreateLinkArgs {
            code: "EXPIRD",
            expires_at: past, // already expired
            created_at: past - 3600,
            ..make_args("EXPIRD", past - 3600, 1)
        };
        create_link(&conn, &args).unwrap();
        assert!(matches!(resolve_link(&conn, "EXPIRD").unwrap(), ResolveResult::Expired));
    }

    #[test]
    fn sweep_removes_only_expired() {
        let (_dir, conn) = open_temp_db();
        let now = unix_now();
        create_link(&conn, &make_args("LIVE01", now, 3600)).unwrap();
        create_link(&conn, &CreateLinkArgs {
            code: "DEAD01",
            expires_at: now - 1,
            created_at: now - 3601,
            ..make_args("DEAD01", now - 3601, 1)
        }).unwrap();
        let removed = sweep_expired(&conn).unwrap();
        assert_eq!(removed, 1);
        assert!(matches!(resolve_link(&conn, "LIVE01").unwrap(), ResolveResult::Found(_)));
        assert!(matches!(resolve_link(&conn, "DEAD01").unwrap(), ResolveResult::NotFound));
    }

    #[test]
    fn ttl_string_parsing() {
        use find_common::config::parse_ttl;
        assert_eq!(parse_ttl("30d").unwrap(), 30 * 24 * 3600);
        assert_eq!(parse_ttl("7d").unwrap(), 7 * 24 * 3600);
        assert_eq!(parse_ttl("24h").unwrap(), 86400);
        assert_eq!(parse_ttl("1h").unwrap(), 3600);
        assert!(parse_ttl("invalid").is_err());
        assert!(parse_ttl("30x").is_err());
    }
}
