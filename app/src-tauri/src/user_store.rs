use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// User settings stored in SQLite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub id: i64,
    pub theme: String,                    // "dark", "light", "system"
    pub llm_url: String,                  // LLM API endpoint
    pub llm_model: String,                // Model name
    pub llm_api_key: String,              // API key for LLM (optional for local servers)
    pub auto_record: bool,                // Auto-start recording on meeting
    pub notifications_enabled: bool,
    pub language: String,                 // "en", "es", etc.
    pub created_at: String,
    pub updated_at: String,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            id: 1,
            theme: "system".to_string(),
            llm_url: String::new(),  // Empty by default - user must configure
            llm_model: String::new(),
            llm_api_key: String::new(),  // Empty for local servers
            auto_record: false,
            notifications_enabled: true,
            language: "en".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

/// Quick note (not tied to meetings)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: i64,
    pub content: String,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Integration/tool connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Integration {
    pub id: String,                       // "google_calendar", "slack", etc.
    pub name: String,
    pub status: String,                   // "connected", "disconnected", "pending"
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<String>,
    pub metadata: Option<String>,         // JSON blob for extra data
    pub connected_at: Option<String>,
}

/// Saved search query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSearch {
    pub id: i64,
    pub query: String,
    pub name: String,
    pub created_at: String,
}

/// The user data store backed by SQLite
pub struct UserStore {
    conn: Connection,
}

impl UserStore {
    /// Open or create the user store database
    pub fn new(data_dir: &PathBuf) -> Result<Self, String> {
        let db_path = data_dir.join("user_store.db");

        // Ensure directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create data directory: {}", e))?;
        }

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open user store: {}", e))?;

        let store = Self { conn };
        store.init_schema()?;

        println!("User store initialized at {:?}", db_path);
        Ok(store)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<(), String> {
        self.conn.execute_batch(r#"
            -- User settings (singleton table)
            CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                theme TEXT NOT NULL DEFAULT 'system',
                llm_url TEXT NOT NULL DEFAULT '',
                llm_model TEXT NOT NULL DEFAULT '',
                llm_api_key TEXT NOT NULL DEFAULT '',
                auto_record INTEGER NOT NULL DEFAULT 0,
                notifications_enabled INTEGER NOT NULL DEFAULT 1,
                language TEXT NOT NULL DEFAULT 'en',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Insert default settings if not exists
            INSERT OR IGNORE INTO settings (id) VALUES (1);

            -- Migration: add llm_api_key column if it doesn't exist
            -- SQLite doesn't support IF NOT EXISTS for ALTER TABLE, so we handle errors silently

            -- Quick notes
            CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                tags TEXT NOT NULL DEFAULT '[]',
                pinned INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Integrations/connected tools
            CREATE TABLE IF NOT EXISTS integrations (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'disconnected',
                access_token TEXT,
                refresh_token TEXT,
                expires_at TEXT,
                metadata TEXT,
                connected_at TEXT
            );

            -- Saved searches
            CREATE TABLE IF NOT EXISTS saved_searches (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                query TEXT NOT NULL,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- App state (key-value for misc stuff)
            CREATE TABLE IF NOT EXISTS app_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            -- Create indexes
            CREATE INDEX IF NOT EXISTS idx_notes_pinned ON notes(pinned);
            CREATE INDEX IF NOT EXISTS idx_notes_created ON notes(created_at DESC);
        "#).map_err(|e| format!("Failed to create schema: {}", e))?;

        // Run migrations for existing databases
        self.run_migrations()?;

        Ok(())
    }

    /// Run database migrations
    fn run_migrations(&self) -> Result<(), String> {
        // Add llm_api_key column if it doesn't exist
        let _ = self.conn.execute(
            "ALTER TABLE settings ADD COLUMN llm_api_key TEXT NOT NULL DEFAULT ''",
            [],
        ); // Ignore error if column already exists

        Ok(())
    }

    // ==================== SETTINGS ====================

    /// Get user settings
    pub fn get_settings(&self) -> Result<UserSettings, String> {
        let mut stmt = self.conn
            .prepare("SELECT id, theme, llm_url, llm_model, COALESCE(llm_api_key, '') as llm_api_key, auto_record, notifications_enabled, language, created_at, updated_at FROM settings WHERE id = 1")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let settings = stmt.query_row([], |row| {
            Ok(UserSettings {
                id: row.get(0)?,
                theme: row.get(1)?,
                llm_url: row.get(2)?,
                llm_model: row.get(3)?,
                llm_api_key: row.get(4)?,
                auto_record: row.get::<_, i32>(5)? != 0,
                notifications_enabled: row.get::<_, i32>(6)? != 0,
                language: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        }).map_err(|e| format!("Failed to get settings: {}", e))?;

        Ok(settings)
    }

    /// Update user settings
    pub fn update_settings(&self, settings: &UserSettings) -> Result<(), String> {
        self.conn.execute(
            "UPDATE settings SET theme = ?1, llm_url = ?2, llm_model = ?3, llm_api_key = ?4, auto_record = ?5, notifications_enabled = ?6, language = ?7, updated_at = datetime('now') WHERE id = 1",
            params![
                settings.theme,
                settings.llm_url,
                settings.llm_model,
                settings.llm_api_key,
                settings.auto_record as i32,
                settings.notifications_enabled as i32,
                settings.language,
            ],
        ).map_err(|e| format!("Failed to update settings: {}", e))?;

        Ok(())
    }

    /// Update a single setting
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let valid_keys = ["theme", "llm_url", "llm_model", "llm_api_key", "language"];
        if !valid_keys.contains(&key) {
            return Err(format!("Invalid setting key: {}", key));
        }

        let sql = format!("UPDATE settings SET {} = ?1, updated_at = datetime('now') WHERE id = 1", key);
        self.conn.execute(&sql, params![value])
            .map_err(|e| format!("Failed to set {}: {}", key, e))?;

        Ok(())
    }

    /// Update a boolean setting
    pub fn set_setting_bool(&self, key: &str, value: bool) -> Result<(), String> {
        let valid_keys = ["auto_record", "notifications_enabled"];
        if !valid_keys.contains(&key) {
            return Err(format!("Invalid boolean setting key: {}", key));
        }

        let sql = format!("UPDATE settings SET {} = ?1, updated_at = datetime('now') WHERE id = 1", key);
        self.conn.execute(&sql, params![value as i32])
            .map_err(|e| format!("Failed to set {}: {}", key, e))?;

        Ok(())
    }

    // ==================== NOTES ====================

    /// Create a new note
    pub fn create_note(&self, content: &str, tags: &[String]) -> Result<Note, String> {
        let tags_json = serde_json::to_string(tags)
            .map_err(|e| format!("Failed to serialize tags: {}", e))?;

        self.conn.execute(
            "INSERT INTO notes (content, tags) VALUES (?1, ?2)",
            params![content, tags_json],
        ).map_err(|e| format!("Failed to create note: {}", e))?;

        let id = self.conn.last_insert_rowid();
        self.get_note(id)
    }

    /// Get a note by ID
    pub fn get_note(&self, id: i64) -> Result<Note, String> {
        let mut stmt = self.conn
            .prepare("SELECT id, content, tags, pinned, created_at, updated_at FROM notes WHERE id = ?1")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let note = stmt.query_row(params![id], |row| {
            let tags_json: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            Ok(Note {
                id: row.get(0)?,
                content: row.get(1)?,
                tags,
                pinned: row.get::<_, i32>(3)? != 0,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        }).map_err(|e| format!("Note not found: {}", e))?;

        Ok(note)
    }

    /// Get all notes (optionally limit)
    pub fn get_notes(&self, limit: Option<usize>) -> Result<Vec<Note>, String> {
        let sql = match limit {
            Some(l) => format!("SELECT id, content, tags, pinned, created_at, updated_at FROM notes ORDER BY pinned DESC, created_at DESC LIMIT {}", l),
            None => "SELECT id, content, tags, pinned, created_at, updated_at FROM notes ORDER BY pinned DESC, created_at DESC".to_string(),
        };

        let mut stmt = self.conn.prepare(&sql)
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let notes = stmt.query_map([], |row| {
            let tags_json: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            Ok(Note {
                id: row.get(0)?,
                content: row.get(1)?,
                tags,
                pinned: row.get::<_, i32>(3)? != 0,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        }).map_err(|e| format!("Failed to query notes: {}", e))?;

        notes.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect notes: {}", e))
    }

    /// Update a note
    pub fn update_note(&self, id: i64, content: &str, tags: &[String]) -> Result<Note, String> {
        let tags_json = serde_json::to_string(tags)
            .map_err(|e| format!("Failed to serialize tags: {}", e))?;

        self.conn.execute(
            "UPDATE notes SET content = ?1, tags = ?2, updated_at = datetime('now') WHERE id = ?3",
            params![content, tags_json, id],
        ).map_err(|e| format!("Failed to update note: {}", e))?;

        self.get_note(id)
    }

    /// Toggle note pinned status
    pub fn toggle_note_pin(&self, id: i64) -> Result<Note, String> {
        self.conn.execute(
            "UPDATE notes SET pinned = NOT pinned, updated_at = datetime('now') WHERE id = ?1",
            params![id],
        ).map_err(|e| format!("Failed to toggle pin: {}", e))?;

        self.get_note(id)
    }

    /// Delete a note
    pub fn delete_note(&self, id: i64) -> Result<(), String> {
        self.conn.execute("DELETE FROM notes WHERE id = ?1", params![id])
            .map_err(|e| format!("Failed to delete note: {}", e))?;
        Ok(())
    }

    // ==================== INTEGRATIONS ====================

    /// Get all integrations
    pub fn get_integrations(&self) -> Result<Vec<Integration>, String> {
        let mut stmt = self.conn
            .prepare("SELECT id, name, status, access_token, refresh_token, expires_at, metadata, connected_at FROM integrations")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let integrations = stmt.query_map([], |row| {
            Ok(Integration {
                id: row.get(0)?,
                name: row.get(1)?,
                status: row.get(2)?,
                access_token: row.get(3)?,
                refresh_token: row.get(4)?,
                expires_at: row.get(5)?,
                metadata: row.get(6)?,
                connected_at: row.get(7)?,
            })
        }).map_err(|e| format!("Failed to query integrations: {}", e))?;

        integrations.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect integrations: {}", e))
    }

    /// Upsert an integration
    pub fn upsert_integration(&self, integration: &Integration) -> Result<(), String> {
        self.conn.execute(
            r#"
            INSERT INTO integrations (id, name, status, access_token, refresh_token, expires_at, metadata, connected_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                status = excluded.status,
                access_token = excluded.access_token,
                refresh_token = excluded.refresh_token,
                expires_at = excluded.expires_at,
                metadata = excluded.metadata,
                connected_at = excluded.connected_at
            "#,
            params![
                integration.id,
                integration.name,
                integration.status,
                integration.access_token,
                integration.refresh_token,
                integration.expires_at,
                integration.metadata,
                integration.connected_at,
            ],
        ).map_err(|e| format!("Failed to upsert integration: {}", e))?;

        Ok(())
    }

    /// Disconnect an integration
    pub fn disconnect_integration(&self, id: &str) -> Result<(), String> {
        self.conn.execute(
            "UPDATE integrations SET status = 'disconnected', access_token = NULL, refresh_token = NULL, expires_at = NULL WHERE id = ?1",
            params![id],
        ).map_err(|e| format!("Failed to disconnect integration: {}", e))?;

        Ok(())
    }

    // ==================== SAVED SEARCHES ====================

    /// Save a search query
    pub fn save_search(&self, query: &str, name: &str) -> Result<SavedSearch, String> {
        self.conn.execute(
            "INSERT INTO saved_searches (query, name) VALUES (?1, ?2)",
            params![query, name],
        ).map_err(|e| format!("Failed to save search: {}", e))?;

        let id = self.conn.last_insert_rowid();

        let mut stmt = self.conn
            .prepare("SELECT id, query, name, created_at FROM saved_searches WHERE id = ?1")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let search = stmt.query_row(params![id], |row| {
            Ok(SavedSearch {
                id: row.get(0)?,
                query: row.get(1)?,
                name: row.get(2)?,
                created_at: row.get(3)?,
            })
        }).map_err(|e| format!("Failed to get saved search: {}", e))?;

        Ok(search)
    }

    /// Get all saved searches
    pub fn get_saved_searches(&self) -> Result<Vec<SavedSearch>, String> {
        let mut stmt = self.conn
            .prepare("SELECT id, query, name, created_at FROM saved_searches ORDER BY created_at DESC")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let searches = stmt.query_map([], |row| {
            Ok(SavedSearch {
                id: row.get(0)?,
                query: row.get(1)?,
                name: row.get(2)?,
                created_at: row.get(3)?,
            })
        }).map_err(|e| format!("Failed to query saved searches: {}", e))?;

        searches.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect saved searches: {}", e))
    }

    /// Delete a saved search
    pub fn delete_saved_search(&self, id: i64) -> Result<(), String> {
        self.conn.execute("DELETE FROM saved_searches WHERE id = ?1", params![id])
            .map_err(|e| format!("Failed to delete saved search: {}", e))?;
        Ok(())
    }

    // ==================== APP STATE (Key-Value) ====================

    /// Get app state value
    pub fn get_state(&self, key: &str) -> Result<Option<String>, String> {
        let mut stmt = self.conn
            .prepare("SELECT value FROM app_state WHERE key = ?1")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let result = stmt.query_row(params![key], |row| row.get(0));

        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Failed to get state: {}", e)),
        }
    }

    /// Set app state value
    pub fn set_state(&self, key: &str, value: &str) -> Result<(), String> {
        self.conn.execute(
            "INSERT INTO app_state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        ).map_err(|e| format!("Failed to set state: {}", e))?;

        Ok(())
    }

    /// Delete app state value
    pub fn delete_state(&self, key: &str) -> Result<(), String> {
        self.conn.execute("DELETE FROM app_state WHERE key = ?1", params![key])
            .map_err(|e| format!("Failed to delete state: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn test_settings() {
        let dir = temp_dir();
        let store = UserStore::new(&dir).unwrap();

        // Get default settings
        let settings = store.get_settings().unwrap();
        assert_eq!(settings.theme, "system");

        // Update a setting
        store.set_setting("theme", "dark").unwrap();
        let settings = store.get_settings().unwrap();
        assert_eq!(settings.theme, "dark");
    }

    #[test]
    fn test_notes() {
        let dir = temp_dir();
        let store = UserStore::new(&dir).unwrap();

        // Create note
        let note = store.create_note("Test note", &["tag1".to_string(), "tag2".to_string()]).unwrap();
        assert_eq!(note.content, "Test note");
        assert_eq!(note.tags.len(), 2);

        // Get notes
        let notes = store.get_notes(None).unwrap();
        assert!(!notes.is_empty());

        // Update note
        let updated = store.update_note(note.id, "Updated content", &["new_tag".to_string()]).unwrap();
        assert_eq!(updated.content, "Updated content");

        // Delete note
        store.delete_note(note.id).unwrap();
    }

    #[test]
    fn test_app_state() {
        let dir = temp_dir();
        let store = UserStore::new(&dir).unwrap();

        // Set and get
        store.set_state("last_view", "meetings").unwrap();
        let value = store.get_state("last_view").unwrap();
        assert_eq!(value, Some("meetings".to_string()));

        // Non-existent key
        let missing = store.get_state("nonexistent").unwrap();
        assert_eq!(missing, None);
    }
}
