//! The vault: an SQLCipher database plus a small metadata file holding the
//! wrapped data key.
//!
//! Files (inside the app's data directory, permissions 0600):
//! * `vault.json` - format version and the two key slots (password, recovery)
//! * `vault.db`   - SQLCipher database, AES-256 encrypted
//!
//! An unlocked vault is represented by a [`Session`]. Dropping the session
//! closes the database; SQLCipher wipes its key from memory on close.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::crypto::{self, KdfParams, KeySlot, SecretKey};
use crate::db;
use crate::error::{CoreError, Result};
use crate::password;
use crate::recovery;

const META_FILE: &str = "vault.json";
const DB_FILE: &str = "vault.db";
const FORMAT: &str = "finance-app-vault";
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultMeta {
    format: String,
    version: u32,
    created_at: String,
    password_slot: KeySlot,
    recovery_slot: KeySlot,
}

/// Result of creating a vault or using the recovery key: the open session and
/// the (new) recovery key to show the user exactly once.
pub struct Opened {
    pub session: Session,
    pub recovery_key: Zeroizing<String>,
}

pub struct Vault;

impl Vault {
    pub fn exists(dir: &Path) -> bool {
        dir.join(META_FILE).is_file()
    }

    /// Create a new vault protected by `password`.
    pub fn create(dir: &Path, password: &str) -> Result<Opened> {
        Self::create_with(dir, password, KdfParams::DEFAULT)
    }

    pub(crate) fn create_with(dir: &Path, pw: &str, kdf: KdfParams) -> Result<Opened> {
        password::validate(pw)?;
        if Self::exists(dir) {
            return Err(CoreError::VaultExists);
        }
        ensure_private_dir(dir)?;
        // A database without metadata is unusable (its key is gone), e.g.
        // left over from an interrupted setup. Remove it before starting over.
        remove_db_files(dir)?;

        let dek = crypto::random_key();
        let (recovery_raw, recovery_text) = recovery::generate();
        let meta = VaultMeta {
            format: FORMAT.into(),
            version: FORMAT_VERSION,
            created_at: now(),
            password_slot: crypto::seal_slot(pw.as_bytes(), &dek, kdf)?,
            recovery_slot: crypto::seal_slot(recovery_raw.as_ref(), &dek, kdf)?,
        };

        // Initialise the database first; write metadata last so a crash
        // mid-way never leaves a vault.json pointing at a missing database.
        let conn = open_db(dir, &dek, true)?;
        write_meta(dir, &meta)?;
        Ok(Opened { session: Session { conn, dir: dir.to_path_buf() }, recovery_key: recovery_text })
    }

    /// Unlock with the master password.
    pub fn unlock(dir: &Path, pw: &str) -> Result<Session> {
        let meta = read_meta(dir)?;
        let dek = crypto::open_slot(pw.as_bytes(), &meta.password_slot)?;
        let conn = open_db(dir, &dek, false)?;
        Ok(Session { conn, dir: dir.to_path_buf() })
    }

    /// Forgotten password: unlock with the recovery key, set a new password,
    /// and issue a fresh recovery key (the used one is retired).
    pub fn recover(dir: &Path, recovery_key: &str, new_password: &str) -> Result<Opened> {
        Self::recover_with(dir, recovery_key, new_password, KdfParams::DEFAULT)
    }

    pub(crate) fn recover_with(
        dir: &Path,
        recovery_key: &str,
        new_password: &str,
        kdf: KdfParams,
    ) -> Result<Opened> {
        password::validate(new_password)?;
        let mut meta = read_meta(dir)?;
        let raw = recovery::parse(recovery_key)?;
        let dek = crypto::open_slot(raw.as_ref(), &meta.recovery_slot)
            .map_err(|e| match e {
                CoreError::WrongPassword => CoreError::InvalidRecoveryKey,
                other => other,
            })?;
        let conn = open_db(dir, &dek, false)?;
        let (new_raw, new_text) = recovery::generate();
        meta.password_slot = crypto::seal_slot(new_password.as_bytes(), &dek, kdf)?;
        meta.recovery_slot = crypto::seal_slot(new_raw.as_ref(), &dek, kdf)?;
        write_meta(dir, &meta)?;
        Ok(Opened { session: Session { conn, dir: dir.to_path_buf() }, recovery_key: new_text })
    }
}

/// An unlocked vault. Drop it to lock.
pub struct Session {
    conn: Connection,
    dir: PathBuf,
}

impl Session {
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Check the master password without changing anything. Used to confirm
    /// actions that deserve a second thought, such as switching to live bank
    /// data.
    pub fn verify_password(&self, password: &str) -> Result<()> {
        let meta = read_meta(&self.dir)?;
        crypto::open_slot(password.as_bytes(), &meta.password_slot)?;
        Ok(())
    }

    /// Re-wrap the data key under a new password. The database itself is not
    /// re-encrypted, so this is instant regardless of data size.
    pub fn change_password(&self, old: &str, new: &str) -> Result<()> {
        self.change_password_with(old, new, KdfParams::DEFAULT)
    }

    pub(crate) fn change_password_with(&self, old: &str, new: &str, kdf: KdfParams) -> Result<()> {
        password::validate(new)?;
        if old == new {
            return Err(CoreError::WeakPassword("The new password must be different.".into()));
        }
        let mut meta = read_meta(&self.dir)?;
        let dek = crypto::open_slot(old.as_bytes(), &meta.password_slot)?;
        meta.password_slot = crypto::seal_slot(new.as_bytes(), &dek, kdf)?;
        write_meta(&self.dir, &meta)
    }

    pub fn lock(self) {
        drop(self);
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn open_db(dir: &Path, dek: &SecretKey, create: bool) -> Result<Connection> {
    let path = dir.join(DB_FILE);
    if !create && !path.is_file() {
        return Err(CoreError::CorruptVault);
    }
    let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if create {
        flags |= OpenFlags::SQLITE_OPEN_CREATE;
    }
    let mut conn = Connection::open_with_flags(&path, flags)?;

    // SQLCipher raw-key syntax: PRAGMA key = "x'<64 hex chars>'".
    // Supplying a raw key skips SQLCipher's own PBKDF2 step; our key is
    // already random 256-bit material.
    let key_literal = Zeroizing::new(format!("x'{}'", hex::encode(dek.as_ref())));
    conn.pragma_update(None, "key", key_literal.as_str())?;
    // Wipe SQLCipher's internal buffers when they are freed.
    conn.pragma_update(None, "cipher_memory_security", "ON")?;

    // Reading the schema is the first operation that actually decrypts a
    // page; a wrong key fails here with SQLITE_NOTADB.
    let check: rusqlite::Result<i64> =
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0));
    if let Err(e) = check {
        return Err(match e.sqlite_error_code() {
            Some(rusqlite::ErrorCode::NotADatabase) => CoreError::CorruptVault,
            _ => CoreError::Db(e),
        });
    }

    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "secure_delete", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    db::migrate(&mut conn)?;
    restrict_permissions(&path)?;
    Ok(conn)
}

fn read_meta(dir: &Path) -> Result<VaultMeta> {
    let path = dir.join(META_FILE);
    if !path.is_file() {
        return Err(CoreError::VaultMissing);
    }
    let meta: VaultMeta =
        serde_json::from_slice(&fs::read(&path)?).map_err(|_| CoreError::CorruptVault)?;
    if meta.format != FORMAT || meta.version != FORMAT_VERSION {
        return Err(CoreError::CorruptVault);
    }
    Ok(meta)
}

/// Atomic write: temp file, fsync, rename.
fn write_meta(dir: &Path, meta: &VaultMeta) -> Result<()> {
    let tmp = dir.join(format!("{META_FILE}.tmp"));
    let data = serde_json::to_vec_pretty(meta)?;
    {
        let mut opts = OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(&data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, dir.join(META_FILE))?;
    restrict_permissions(&dir.join(META_FILE))?;
    Ok(())
}

fn ensure_private_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn restrict_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    let _ = path;
    Ok(())
}

fn remove_db_files(dir: &Path) -> Result<()> {
    for name in [DB_FILE, "vault.db-journal", "vault.db-wal", "vault.db-shm"] {
        let p = dir.join(name);
        if p.exists() {
            fs::remove_file(p)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings;

    const PW: &str = "maple river lantern 42";
    const PW2: &str = "copper falcon harbor 77";
    const T: KdfParams = KdfParams::TEST;

    fn new_vault() -> (tempfile::TempDir, Opened) {
        let dir = tempfile::tempdir().unwrap();
        let opened = Vault::create_with(dir.path(), PW, T).unwrap();
        (dir, opened)
    }

    #[test]
    fn create_unlock_and_persist_data() {
        let (dir, opened) = new_vault();
        settings::set_auto_lock_minutes(opened.session.conn(), 9).unwrap();
        opened.session.lock();

        assert!(Vault::exists(dir.path()));
        let s = Vault::unlock(dir.path(), PW).unwrap();
        assert_eq!(settings::load(s.conn()).unwrap().auto_lock_minutes, 9);
    }

    #[test]
    fn wrong_password_fails() {
        let (dir, opened) = new_vault();
        drop(opened);
        assert!(matches!(Vault::unlock(dir.path(), "nope nope nope"), Err(CoreError::WrongPassword)));
    }

    #[test]
    fn cannot_create_twice() {
        let (dir, _opened) = new_vault();
        assert!(matches!(Vault::create_with(dir.path(), PW, T), Err(CoreError::VaultExists)));
    }

    #[test]
    fn weak_password_rejected_at_create() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(Vault::create_with(dir.path(), "short", T), Err(CoreError::WeakPassword(_))));
        assert!(!Vault::exists(dir.path()));
    }

    #[test]
    fn database_file_is_encrypted() {
        let (dir, opened) = new_vault();
        settings::set(opened.session.conn(), "canary", "PLAINTEXT-CANARY-123").unwrap();
        drop(opened);
        let bytes = fs::read(dir.path().join(DB_FILE)).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3\0"), "header must not be plaintext");
        let needle = b"PLAINTEXT-CANARY-123";
        assert!(!bytes.windows(needle.len()).any(|w| w == needle), "data must not be plaintext");

        // Opening without the key must fail.
        let conn = Connection::open(dir.path().join(DB_FILE)).unwrap();
        let r: rusqlite::Result<i64> =
            conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0));
        assert!(r.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, _opened) = new_vault();
        for f in [META_FILE, DB_FILE] {
            let mode = fs::metadata(dir.path().join(f)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{f}");
        }
    }

    #[test]
    fn verify_password_checks_without_changing_anything() {
        let (dir, opened) = new_vault();
        opened.session.verify_password(PW).unwrap();
        assert!(matches!(opened.session.verify_password(PW2), Err(CoreError::WrongPassword)));
        drop(opened);
        // Still unlockable with the original password.
        Vault::unlock(dir.path(), PW).unwrap();
    }

    #[test]
    fn change_password() {
        let (dir, opened) = new_vault();
        assert!(opened.session.change_password_with("wrong wrong wrong", PW2, T).is_err());
        opened.session.change_password_with(PW, PW2, T).unwrap();
        drop(opened);
        assert!(Vault::unlock(dir.path(), PW).is_err());
        assert!(Vault::unlock(dir.path(), PW2).is_ok());
    }

    #[test]
    fn recovery_resets_password_and_rotates_key() {
        let (dir, opened) = new_vault();
        let first_key = opened.recovery_key.to_string();
        settings::set(opened.session.conn(), "k", "v").unwrap();
        drop(opened);

        assert!(matches!(
            Vault::recover_with(dir.path(), "AAAA-BBBB", PW2, T),
            Err(CoreError::InvalidRecoveryKey)
        ));
        let (_, other_key) = recovery::generate();
        assert!(matches!(
            Vault::recover_with(dir.path(), &other_key, PW2, T),
            Err(CoreError::InvalidRecoveryKey)
        ));

        let rec = Vault::recover_with(dir.path(), &first_key, PW2, T).unwrap();
        assert_eq!(settings::get(rec.session.conn(), "k").unwrap().as_deref(), Some("v"));
        let second_key = rec.recovery_key.to_string();
        assert_ne!(first_key, second_key);
        drop(rec);

        assert!(Vault::unlock(dir.path(), PW).is_err());
        assert!(Vault::unlock(dir.path(), PW2).is_ok());
        // The used recovery key is retired.
        assert!(Vault::recover_with(dir.path(), &first_key, PW, T).is_err());
        assert!(Vault::recover_with(dir.path(), &second_key, PW, T).is_ok());
    }

    #[test]
    fn tampered_meta_is_rejected() {
        let (dir, opened) = new_vault();
        drop(opened);
        fs::write(dir.path().join(META_FILE), b"{not json").unwrap();
        assert!(matches!(Vault::unlock(dir.path(), PW), Err(CoreError::CorruptVault)));
    }

    #[test]
    fn missing_vault_reports_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(Vault::unlock(dir.path(), PW), Err(CoreError::VaultMissing)));
    }
}
