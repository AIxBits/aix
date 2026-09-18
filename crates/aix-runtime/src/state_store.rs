//! Persistent app-local state storage.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Storage failure returned without exposing SQL statements or secrets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateStoreError {
    /// Stable error category.
    pub code: String,
    /// Human-readable diagnostic.
    pub message: String,
}

impl std::fmt::Display for StateStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for StateStoreError {}

/// Persistence boundary used by an app session.
pub trait StateStore {
    /// Load the last state for an app, or `None` on first launch.
    fn load(&self, app_id: &str) -> Result<Option<Value>, StateStoreError>;

    /// Atomically replace the persisted state for an app.
    fn save(&self, app_id: &str, state: &Value) -> Result<(), StateStoreError>;
}

/// In-memory store useful for embedding and deterministic tests.
#[derive(Default)]
pub struct MemoryStateStore {
    states: std::sync::Mutex<BTreeMap<String, Value>>,
}

impl MemoryStateStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl StateStore for MemoryStateStore {
    fn load(&self, app_id: &str) -> Result<Option<Value>, StateStoreError> {
        Ok(self
            .states
            .lock()
            .expect("state mutex poisoned")
            .get(app_id)
            .cloned())
    }

    fn save(&self, app_id: &str, state: &Value) -> Result<(), StateStoreError> {
        self.states
            .lock()
            .expect("state mutex poisoned")
            .insert(app_id.to_owned(), state.clone());
        Ok(())
    }
}

/// SQLite implementation used by the desktop host.
pub struct SqliteStateStore {
    connection: Connection,
}

impl SqliteStateStore {
    /// Open or create a state database at a filesystem path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StateStoreError> {
        let connection = Connection::open(path).map_err(sqlite_error)?;
        Self::from_connection(connection)
    }

    /// Create a transient SQLite database for tests or ephemeral sessions.
    pub fn in_memory() -> Result<Self, StateStoreError> {
        let connection = Connection::open_in_memory().map_err(sqlite_error)?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self, StateStoreError> {
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS aix_app_state (
                    app_id TEXT PRIMARY KEY NOT NULL,
                    state_json TEXT NOT NULL,
                    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
                );",
            )
            .map_err(sqlite_error)?;
        Ok(Self { connection })
    }
}

impl StateStore for SqliteStateStore {
    fn load(&self, app_id: &str) -> Result<Option<Value>, StateStoreError> {
        let encoded = self
            .connection
            .query_row(
                "SELECT state_json FROM aix_app_state WHERE app_id = ?1",
                params![app_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sqlite_error)?;
        encoded
            .map(|encoded| {
                serde_json::from_str(&encoded).map_err(|error| StateStoreError {
                    code: "invalid_persisted_state".to_owned(),
                    message: format!("stored state is not valid JSON: {error}"),
                })
            })
            .transpose()
    }

    fn save(&self, app_id: &str, state: &Value) -> Result<(), StateStoreError> {
        let encoded = serde_json::to_string(state).map_err(|error| StateStoreError {
            code: "state_serialization_failed".to_owned(),
            message: format!("state could not be serialized: {error}"),
        })?;
        self.connection
            .execute(
                "INSERT INTO aix_app_state (app_id, state_json, updated_at)
                 VALUES (?1, ?2, unixepoch())
                 ON CONFLICT(app_id) DO UPDATE SET
                    state_json = excluded.state_json,
                    updated_at = excluded.updated_at",
                params![app_id, encoded],
            )
            .map_err(sqlite_error)?;
        Ok(())
    }
}

fn sqlite_error(error: rusqlite::Error) -> StateStoreError {
    StateStoreError {
        code: "sqlite_error".to_owned(),
        message: error.to_string(),
    }
}
