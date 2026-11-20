//! The one writer of a database. One thread owns the connection and the table modules; every
//! request is one transaction. The authorizer lets the module that executes a request write
//! only the tables it declared; everything else in that transaction is refused and rolled
//! back. When no request comes for a while the connection is released, so a closed project
//! does not stay held, and opened again at the next request.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::Connection;
use serde_json::Value;

use crate::TableModule;

pub const IDLE_RELEASE: Duration = Duration::from_secs(2);

struct Request {
    module: String,
    value: Value,
    reply: mpsc::Sender<Result<Value, String>>,
}

/// The handle to a database's writer; clone it for every writer-side user.
#[derive(Clone)]
pub struct Writer {
    path: PathBuf,
    requests: mpsc::Sender<Request>,
}

/// A reply that is on its way.
pub struct Pending(mpsc::Receiver<Result<Value, String>>);

impl Pending {
    /// Waits for the reply.
    pub fn wait(self) -> Result<Value, String> {
        self.0.recv().unwrap_or_else(|_| Err(qnc_text::text("db.closed").to_string()))
    }

    /// The reply if it has arrived, without waiting.
    pub fn take(&self) -> Option<Result<Value, String>> {
        self.0.try_recv().ok()
    }
}

impl Writer {
    /// Starts the writer of an existing database with its table modules. The modules' tables are
    /// brought up before the first request is served.
    pub fn start(path: &Path, modules: Vec<Box<dyn TableModule>>) -> Result<Writer, String> {
        let (requests, inbox) = mpsc::channel::<Request>();
        let file = path.to_path_buf();
        let conn = crate::open_existing(&file)?;
        let mut modules = modules;
        attach_all(&conn, &mut modules)?;
        thread::Builder::new()
            .name(format!("qnc-db-writer {}", file.display()))
            .spawn(move || {
                let mut held = Some(conn);
                loop {
                    let request = match inbox.recv_timeout(IDLE_RELEASE) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            held = None;
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    };
                    let connection = match held.take().map(Ok).unwrap_or_else(|| crate::open_existing(&file)) {
                        Ok(connection) => connection,
                        Err(error) => {
                            let _ = request.reply.send(Err(error));
                            continue;
                        }
                    };
                    let outcome = serve(&connection, &mut modules, &request.module, request.value);
                    let _ = request.reply.send(outcome);
                    held = Some(connection);
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Writer { path: path.to_path_buf(), requests })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Sends a request to a module and does not wait.
    pub fn submit(&self, module: &str, value: Value) -> Pending {
        let (reply, receiver) = mpsc::channel();
        let request = Request { module: module.to_string(), value, reply: reply.clone() };
        if self.requests.send(request).is_err() {
            let _ = reply.send(Err(qnc_text::text("db.closed").to_string()));
        }
        Pending(receiver)
    }

    /// Sends a request to a module and waits for its reply.
    pub fn execute(&self, module: &str, value: Value) -> Result<Value, String> {
        self.submit(module, value).wait()
    }
}

fn attach_all(conn: &Connection, modules: &mut [Box<dyn TableModule>]) -> Result<(), String> {
    conn.execute_batch("BEGIN IMMEDIATE").map_err(|e| e.to_string())?;
    for module in modules.iter_mut() {
        if let Err(error) = module.attach(conn) {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(qnc_text::fill("db.refused", &[("module", module.id()), ("error", &error)]));
        }
    }
    conn.execute_batch("COMMIT").map_err(|e| e.to_string())
}

fn serve(conn: &Connection, modules: &mut [Box<dyn TableModule>], id: &str, value: Value) -> Result<Value, String> {
    let module = modules
        .iter_mut()
        .find(|m| m.id() == id)
        .ok_or_else(|| qnc_text::fill("db.no_module", &[("module", id)]))?;
    conn.execute_batch("BEGIN IMMEDIATE").map_err(|e| e.to_string())?;
    let tables = module.tables();
    conn.authorizer(Some(move |ctx: AuthContext<'_>| own_tables_only(ctx, tables)));
    let outcome = module.execute(conn, value);
    conn.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    match outcome {
        Ok(reply) => {
            conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            Ok(reply)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(qnc_text::fill("db.refused", &[("module", id), ("error", &error)]))
        }
    }
}

fn own_tables_only(ctx: AuthContext<'_>, tables: &'static [&'static str]) -> Authorization {
    match ctx.action {
        AuthAction::Insert { table_name } | AuthAction::Delete { table_name } | AuthAction::Update { table_name, .. } => {
            if tables.contains(&table_name) {
                Authorization::Allow
            } else {
                Authorization::Deny
            }
        }
        AuthAction::Read { .. }
        | AuthAction::Select
        | AuthAction::Transaction { .. }
        | AuthAction::Savepoint { .. }
        | AuthAction::Function { .. }
        | AuthAction::Recursive => Authorization::Allow,
        _ => Authorization::Deny,
    }
}
