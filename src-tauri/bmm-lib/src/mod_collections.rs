//! Mod collection management for grouping and organizing mods.
//!
//! This module provides functionality to create, store, and manage
//! collections of mods. Collections allow users to save and share
//! curated sets of mods.
//!
//! # Features
//!
//! - Create named collections with unique hash identifiers
//! - Persist collections to SQLite database
//! - Load and query collections
//!
//! # Example
//!
//! ```ignore
//! let conn = Database::new()?.connection();
//! let mut manager = ModCollectionManager::new();
//! ModCollectionManager::initialize_table(&conn)?;
//!
//! let collection = ModCollection::new("My Favorites".to_string(), PathBuf::from("/mods"));
//! manager.add_collection(&conn, collection)?;
//! ```

use rusqlite::{Connection, Result};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

#[derive(Clone)]
pub struct ModCollection {
    pub name: String,
    pub path: PathBuf,
    pub hash: u64,
}

impl ModCollection {
    pub fn new(name: String, path: PathBuf) -> Self {
        use std::hash::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        path.to_string_lossy().hash(&mut hasher);
        let hash = hasher.finish();

        Self { name, path, hash }
    }
}

pub struct ModCollectionManager {
    collections: HashMap<u64, ModCollection>,
}

impl Default for ModCollectionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ModCollectionManager {
    pub fn new() -> Self {
        Self {
            collections: HashMap::new(),
        }
    }

    pub fn initialize_table(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS mod_collections (
                hash INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    pub fn add_collection(&mut self, conn: &Connection, collection: ModCollection) -> Result<()> {
        self.collections.insert(collection.hash, collection.clone());

        conn.execute(
            "INSERT OR REPLACE INTO mod_collections (hash, name, path) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                collection.hash as i64,
                &collection.name,
                &collection.path.to_string_lossy().to_string(),
            ],
        )?;

        Ok(())
    }

    pub fn get_collection(&self, hash: u64) -> Option<&ModCollection> {
        self.collections.get(&hash)
    }

    pub fn remove_collection(&mut self, conn: &Connection, hash: u64) -> Result<()> {
        self.collections.remove(&hash);
        conn.execute("DELETE FROM mod_collections WHERE hash = ?1", [hash as i64])?;
        Ok(())
    }

    pub fn load_collections(&mut self, conn: &Connection) -> Result<Vec<ModCollection>> {
        let mut stmt = conn.prepare("SELECT hash, name, path FROM mod_collections")?;

        let collections = stmt.query_map([], |row| {
            let hash: i64 = row.get(0)?;
            let name: String = row.get(1)?;
            let path: String = row.get(2)?;

            Ok(ModCollection {
                hash: hash as u64,
                name,
                path: PathBuf::from(path),
            })
        })?;

        let mut result = Vec::new();
        for collection in collections {
            let collection = collection?;
            self.collections.insert(collection.hash, collection.clone());
            result.push(collection);
        }

        Ok(result)
    }

    pub fn get_all_collections(&self, conn: &Connection) -> Result<Vec<ModCollection>> {
        let mut stmt = conn.prepare("SELECT hash, name, path FROM mod_collections")?;

        let collections = stmt.query_map([], |row| {
            let hash: i64 = row.get(0)?;
            let name: String = row.get(1)?;
            let path: String = row.get(2)?;

            Ok(ModCollection {
                hash: hash as u64,
                name,
                path: PathBuf::from(path),
            })
        })?;

        let mut result = Vec::new();
        for collection in collections {
            result.push(collection?);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        ModCollectionManager::initialize_table(&conn).unwrap();
        conn
    }

    #[test]
    fn test_mod_collection_new_creates_consistent_hash() {
        let col1 = ModCollection::new("TestMod".to_string(), PathBuf::from("/path/to/mod"));
        let col2 = ModCollection::new("TestMod".to_string(), PathBuf::from("/path/to/mod"));
        assert_eq!(col1.hash, col2.hash);
    }

    #[test]
    fn test_mod_collection_different_names_different_hash() {
        let col1 = ModCollection::new("ModA".to_string(), PathBuf::from("/path"));
        let col2 = ModCollection::new("ModB".to_string(), PathBuf::from("/path"));
        assert_ne!(col1.hash, col2.hash);
    }

    #[test]
    fn test_mod_collection_different_paths_different_hash() {
        let col1 = ModCollection::new("Mod".to_string(), PathBuf::from("/path/a"));
        let col2 = ModCollection::new("Mod".to_string(), PathBuf::from("/path/b"));
        assert_ne!(col1.hash, col2.hash);
    }

    #[test]
    fn test_initialize_table_creates_table() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(ModCollectionManager::initialize_table(&conn).is_ok());

        // Verify table exists by querying it
        let result: Result<i64> =
            conn.query_row("SELECT COUNT(*) FROM mod_collections", [], |row| row.get(0));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_initialize_table_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(ModCollectionManager::initialize_table(&conn).is_ok());
        assert!(ModCollectionManager::initialize_table(&conn).is_ok());
    }

    #[test]
    fn test_add_and_get_collection() {
        let conn = setup_test_db();
        let mut manager = ModCollectionManager::new();

        let collection = ModCollection::new("TestMod".to_string(), PathBuf::from("/mods/test"));
        let hash = collection.hash;

        manager.add_collection(&conn, collection).unwrap();

        let retrieved = manager.get_collection(hash);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "TestMod");
    }

    #[test]
    fn test_get_nonexistent_collection_returns_none() {
        let manager = ModCollectionManager::new();
        assert!(manager.get_collection(12345).is_none());
    }

    #[test]
    fn test_remove_collection() {
        let conn = setup_test_db();
        let mut manager = ModCollectionManager::new();

        let collection = ModCollection::new("ToRemove".to_string(), PathBuf::from("/mods/remove"));
        let hash = collection.hash;

        manager.add_collection(&conn, collection).unwrap();
        assert!(manager.get_collection(hash).is_some());

        manager.remove_collection(&conn, hash).unwrap();
        assert!(manager.get_collection(hash).is_none());

        // Verify removed from database
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM mod_collections WHERE hash = ?1",
                [hash.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_load_collections_from_database() {
        let conn = setup_test_db();
        let mut manager = ModCollectionManager::new();

        let col1 = ModCollection::new("Mod1".to_string(), PathBuf::from("/mods/1"));
        let col2 = ModCollection::new("Mod2".to_string(), PathBuf::from("/mods/2"));

        manager.add_collection(&conn, col1).unwrap();
        manager.add_collection(&conn, col2).unwrap();

        // Create new manager and load from database
        let mut new_manager = ModCollectionManager::new();
        let loaded = new_manager.load_collections(&conn).unwrap();

        assert_eq!(loaded.len(), 2);
        assert!(new_manager.get_collection(loaded[0].hash).is_some());
        assert!(new_manager.get_collection(loaded[1].hash).is_some());
    }

    #[test]
    fn test_get_all_collections() {
        let conn = setup_test_db();
        let mut manager = ModCollectionManager::new();

        let col1 = ModCollection::new("Mod1".to_string(), PathBuf::from("/mods/1"));
        let col2 = ModCollection::new("Mod2".to_string(), PathBuf::from("/mods/2"));

        manager.add_collection(&conn, col1).unwrap();
        manager.add_collection(&conn, col2).unwrap();

        let all = manager.get_all_collections(&conn).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_add_collection_replaces_existing() {
        let conn = setup_test_db();
        let mut manager = ModCollectionManager::new();

        let col1 = ModCollection::new("Mod".to_string(), PathBuf::from("/path"));
        let hash = col1.hash;
        manager.add_collection(&conn, col1).unwrap();

        // Add again with same hash (same name+path)
        let col2 = ModCollection::new("Mod".to_string(), PathBuf::from("/path"));
        manager.add_collection(&conn, col2).unwrap();

        // Should still only have one entry
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM mod_collections", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(manager.get_collection(hash).is_some());
    }

    #[test]
    fn test_manager_default_is_empty() {
        let manager = ModCollectionManager::default();
        let conn = setup_test_db();
        let all = manager.get_all_collections(&conn).unwrap();
        assert!(all.is_empty());
    }
}
