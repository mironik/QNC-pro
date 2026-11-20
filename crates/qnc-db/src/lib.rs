//! The database intermediary. A table module owns its tables: it makes them (`attach`) and
//! executes requests on them. The intermediary opens the file (WAL, busy timeout, the WAL file
//! kept so a protected folder never has to delete it), runs every write in one thread inside
//! one `BEGIN IMMEDIATE` transaction, and an authorizer refuses a module's write to any table
//! that is not its own. Reading is a separate read-only connection.

mod writer;

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

pub use writer::{Pending, Writer};

pub const BUSY: Duration = Duration::from_secs(5);

/// A module that owns tables in a database.
pub trait TableModule: Send + 'static {
    /// The id requests name, e.g. `qnc.module.story`.
    fn id(&self) -> &'static str;
    /// The tables this module may write.
    fn tables(&self) -> &'static [&'static str];
    /// Makes or brings up the module's tables. Runs inside the writer's transaction.
    fn attach(&mut self, conn: &Connection) -> Result<(), String>;
    /// Executes one request inside one transaction; an `Err` rolls it back.
    fn execute(&mut self, conn: &Connection, request: Value) -> Result<Value, String>;
}

fn open_failed(path: &Path, error: &str) -> String {
    qnc_text::fill("db.open_failed", &[("path", &path.display().to_string()), ("error", error)])
}

fn settle(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.busy_timeout(BUSY)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    keep_wal_file(conn);
    Ok(())
}

/// SQLite deletes the `-wal` file when the last connection closes; a folder protected from
/// deletion then makes it retry for seconds. The file stays.
fn keep_wal_file(conn: &Connection) {
    let mut on: std::os::raw::c_int = 1;
    // SAFETY: the handle is live for the duration of the call and `on` outlives it.
    unsafe {
        rusqlite::ffi::sqlite3_file_control(
            conn.handle(),
            std::ptr::null(),
            rusqlite::ffi::SQLITE_FCNTL_PERSIST_WAL,
            &mut on as *mut _ as *mut std::os::raw::c_void,
        );
    }
}

/// Opens an existing database for writing (the writer's own connection).
fn open_existing(path: &Path) -> Result<Connection, String> {
    if !path.is_file() {
        return Err(qnc_text::fill("db.missing", &[("path", &path.display().to_string())]));
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|e| open_failed(path, &e.to_string()))?;
    settle(&conn).map_err(|e| open_failed(path, &e.to_string()))?;
    Ok(conn)
}

/// Makes a new, empty database file (the owner of a project does this once).
pub fn create(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(open_failed(path, "it already exists"));
    }
    let conn = Connection::open(path).map_err(|e| open_failed(path, &e.to_string()))?;
    settle(&conn).map_err(|e| open_failed(path, &e.to_string()))?;
    Ok(())
}

/// A read-only view of a database for anyone who only reads.
pub struct Reader {
    path: PathBuf,
    conn: Connection,
}

impl Reader {
    pub fn open(path: &Path) -> Result<Reader, String> {
        if !path.is_file() {
            return Err(qnc_text::fill("db.missing", &[("path", &path.display().to_string())]));
        }
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
            .map_err(|e| open_failed(path, &e.to_string()))?;
        conn.busy_timeout(BUSY).map_err(|e| open_failed(path, &e.to_string()))?;
        Ok(Reader { path: path.to_path_buf(), conn })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests;
