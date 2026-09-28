use crate::config::Config;
use anyhow::{Context, Result, bail};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use rusqlite::{Connection, OptionalExtension};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub fn database_path(config_path: &Path, config: &Config) -> PathBuf {
    let path = PathBuf::from(&config.observability.database);
    if path.is_absolute() {
        path
    } else {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    }
}

pub fn open(config_path: &Path, config: &Config) -> Result<Connection> {
    let path = database_path(config_path, config);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let db = Connection::open(&path)
        .with_context(|| format!("cannot open SQLite database {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS credentials (name TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )?;
    if db
        .query_row(
            "SELECT value FROM credentials WHERE name='client_api_key'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_none()
    {
        let key = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        db.execute(
            "INSERT OR IGNORE INTO credentials(name,value) VALUES('client_api_key',?1)",
            [key],
        )?;
    }
    Ok(db)
}

pub fn client_key(db: &Connection) -> Result<String> {
    Ok(db.query_row(
        "SELECT value FROM credentials WHERE name='client_api_key'",
        [],
        |row| row.get(0),
    )?)
}

pub fn admin_exists(db: &Connection) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM credentials WHERE name='admin_username')",
        [],
        |row| row.get(0),
    )?)
}

pub fn admin_revision(db: &Connection) -> Result<Option<String>> {
    Ok(db
        .query_row(
            "SELECT value FROM credentials WHERE name='admin_password_hash'",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn set_admin(db: &Connection, username: &str, password: &str, reset: bool) -> Result<()> {
    if username.trim() != username
        || username.is_empty()
        || username.len() > 128
        || username.chars().any(char::is_control)
    {
        bail!(
            "admin username must be 1-128 characters without surrounding whitespace or control characters"
        );
    }
    if password.len() < 12 {
        bail!("admin password must be at least 12 bytes");
    }
    if !reset && admin_exists(db)? {
        bail!("admin user already exists; use admin reset-password");
    }
    if reset && !admin_exists(db)? {
        bail!("admin user does not exist; use admin setup");
    }
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes())
        .map_err(|e| anyhow::anyhow!("could not encode password salt: {e}"))?;
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("could not hash password: {e}"))?
        .to_string();
    let tx = db.unchecked_transaction()?;
    tx.execute("INSERT INTO credentials(name,value) VALUES('admin_username',?1) ON CONFLICT(name) DO UPDATE SET value=excluded.value", [username])?;
    tx.execute("INSERT INTO credentials(name,value) VALUES('admin_password_hash',?1) ON CONFLICT(name) DO UPDATE SET value=excluded.value", [hash])?;
    tx.commit()?;
    Ok(())
}

pub fn verify_admin(db: &Connection, username: &str, password: &str) -> Result<bool> {
    let stored: Option<(String,String)> = db.query_row("SELECT u.value,p.value FROM credentials u JOIN credentials p ON p.name='admin_password_hash' WHERE u.name='admin_username'", [], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
    Ok(stored.is_some_and(|(user, hash)| {
        user == username
            && PasswordHash::new(&hash).is_ok_and(|parsed| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_key_persists_and_password_reset_replaces_credentials() {
        let directory =
            std::env::temp_dir().join(format!("gateway-credentials-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.yaml");
        let mut config = crate::config::parse(include_str!("../config.yaml")).unwrap();
        config.observability.database = directory.join("gateway.db").to_string_lossy().into_owned();
        let db = open(&path, &config).unwrap();
        let key = client_key(&db).unwrap();
        assert_eq!(key.len(), 64);
        set_admin(&db, "admin", "initial secret password", false).unwrap();
        assert!(verify_admin(&db, "admin", "initial secret password").unwrap());
        assert!(!verify_admin(&db, "other", "initial secret password").unwrap());
        let revision = admin_revision(&db).unwrap();
        set_admin(&db, "admin", "replacement secret password", true).unwrap();
        assert_ne!(revision, admin_revision(&db).unwrap());
        assert!(!verify_admin(&db, "admin", "initial secret password").unwrap());
        assert!(verify_admin(&db, "admin", "replacement secret password").unwrap());
        drop(db);
        let db = open(&path, &config).unwrap();
        assert_eq!(client_key(&db).unwrap(), key);
        drop(db);
        fs::remove_dir_all(directory).unwrap();
    }
}
