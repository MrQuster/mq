//! Persistent multi-account store (`<data_dir>/accounts.json`) with strict `0600` Unix
//! permissions, atomic write-then-rename durability, 1-click active account switching,
//! and secret token collection for log scrubbing.

use std::fs::{self, OpenOptions};
use std::io::Write;

use mq_core::{AccountId, MqError, MqPaths, MqResult};
use serde::{Deserialize, Serialize};
use tracing::debug;
use uuid::Uuid;

use crate::models::AccountProfile;

/// On-disk JSON schema persisted at `<data_dir>/accounts.json` (`0600` permissions).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountsFileSchema {
    /// Currently selected active account ID for 1-click game launches.
    #[serde(default)]
    pub active_account_id: Option<AccountId>,
    /// Stored player accounts across Offline, Ely.by, and Microsoft providers.
    #[serde(default)]
    pub accounts: Vec<AccountProfile>,
}

/// Multi-account manager backed by `<data_dir>/accounts.json` with strict `0600` file mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountStore {
    #[serde(default)]
    active_account_id: Option<AccountId>,
    #[serde(default)]
    accounts: Vec<AccountProfile>,
}

impl AccountStore {
    /// Creates an empty [`AccountStore`].
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructs an [`AccountStore`] from a deserialized [`AccountsFileSchema`],
    /// normalizing `active_account_id` if it references a missing account.
    pub fn from_schema(schema: AccountsFileSchema) -> Self {
        let mut store = Self {
            active_account_id: schema.active_account_id,
            accounts: schema.accounts,
        };
        store.normalize_active_selection();
        store
    }

    /// Exports the current store state as an [`AccountsFileSchema`].
    pub fn to_schema(&self) -> AccountsFileSchema {
        AccountsFileSchema {
            active_account_id: self.active_account_id,
            accounts: self.accounts.clone(),
        }
    }

    /// Loads `<data_dir>/accounts.json` if present, or returns an empty [`AccountStore`].
    pub fn load_or_default(paths: &MqPaths) -> MqResult<Self> {
        let path = paths.accounts_file();
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&path)?;
        let schema: AccountsFileSchema = serde_json::from_str(&raw)?;
        Ok(Self::from_schema(schema))
    }

    /// Atomically persists the account store to `<data_dir>/accounts.json` via a temporary
    /// file in `paths.data_dir` created with `0o600` mode, enforcing `0o600` permissions
    /// both before and after the atomic rename.
    pub fn save(&self, paths: &MqPaths) -> MqResult<()> {
        let target_path = paths.accounts_file();
        fs::create_dir_all(&paths.data_dir)?;

        let schema = self.to_schema();
        let serialized = serde_json::to_vec_pretty(&schema)?;

        let tmp_name = format!(".accounts.json.{}.tmp", Uuid::new_v4());
        let tmp_path = paths.data_dir.join(tmp_name);

        let write_res = (|| -> std::io::Result<()> {
            let mut opts = OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }

            let mut file = opts.open(&tmp_path)?;
            file.write_all(&serialized)?;
            file.sync_all()?;
            drop(file);

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
            }

            fs::rename(&tmp_path, &target_path)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&target_path, fs::Permissions::from_mode(0o600))?;
            }

            Ok(())
        })();

        if let Err(err) = write_res {
            let _ = fs::remove_file(&tmp_path);
            return Err(MqError::Io(err));
        }

        debug!(
            accounts_count = self.accounts.len(),
            active_account_id = ?self.active_account_id,
            "saved accounts.json with 0600 permissions"
        );
        Ok(())
    }

    /// Inserts or updates `profile` in the store.
    ///
    /// - If an existing account matches `profile.id` or `(profile.kind, profile.username)`
    ///   (case-insensitively), it is updated in place.
    /// - If `make_active` is `true`, or this is the first account in the store, or the
    ///   updated entry was already the active account, `active_account_id` is set to `profile.id`.
    pub fn upsert_account(&mut self, profile: AccountProfile, make_active: bool) {
        let new_id = profile.id;
        let existing_idx = self
            .accounts
            .iter()
            .position(|a| a.id == profile.id)
            .or_else(|| {
                self.accounts.iter().position(|a| {
                    a.kind == profile.kind && a.username.eq_ignore_ascii_case(&profile.username)
                })
            });

        let was_active = match existing_idx {
            Some(idx) => {
                let prev_id = self.accounts[idx].id;
                self.accounts[idx] = profile;
                self.active_account_id == Some(prev_id)
            }
            None => {
                self.accounts.push(profile);
                false
            }
        };

        if make_active || was_active || self.accounts.len() == 1 || self.active_account_id.is_none()
        {
            self.active_account_id = Some(new_id);
        }
    }

    /// Removes the account matching `id_or_name` (by [`AccountId`], username case-insensitively,
    /// or player UUID).
    ///
    /// If the active account was removed, automatically advances `active_account_id` to the
    /// next available account (or `None` when the store becomes empty).
    pub fn remove_account(&mut self, id_or_name: &str) -> bool {
        let Some(idx) = self.find_account_index(id_or_name) else {
            return false;
        };

        let removed = self.accounts.remove(idx);
        if self.active_account_id == Some(removed.id) {
            self.active_account_id = if self.accounts.is_empty() {
                None
            } else {
                let next_idx = idx.min(self.accounts.len() - 1);
                Some(self.accounts[next_idx].id)
            };
        }
        true
    }

    /// 1-click account selector: switches the active account by [`AccountId`] UUID or
    /// `username` (case-insensitively) and returns a reference to the newly active profile.
    pub fn select_account(&mut self, id_or_name: &str) -> MqResult<&AccountProfile> {
        let idx = self.find_account_index(id_or_name).ok_or_else(|| {
            MqError::Auth(format!("account '{id_or_name}' not found in account store"))
        })?;
        let active_id = self.accounts[idx].id;
        self.active_account_id = Some(active_id);
        Ok(&self.accounts[idx])
    }

    /// Returns the currently selected active [`AccountProfile`], if any.
    pub fn active_account(&self) -> Option<&AccountProfile> {
        let active_id = self.active_account_id?;
        self.accounts.iter().find(|a| a.id == active_id)
    }

    /// Returns the currently selected [`AccountId`], if any.
    #[inline]
    pub fn active_account_id(&self) -> Option<AccountId> {
        self.active_account_id
    }

    /// Returns a slice of all stored [`AccountProfile`]s.
    #[inline]
    pub fn list_accounts(&self) -> &[AccountProfile] {
        &self.accounts
    }

    /// Finds an account by [`AccountId`] or `username` (case-insensitively) without mutating selection.
    pub fn find_account(&self, id_or_name: &str) -> Option<&AccountProfile> {
        let idx = self.find_account_index(id_or_name)?;
        self.accounts.get(idx)
    }

    /// Returns the number of stored accounts.
    #[inline]
    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    /// Returns `true` if no accounts are stored.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    /// Collects all non-empty `access_token`, `refresh_token`, and `client_token` secret
    /// slices across every stored account so callers can pass them to [`mq_core::redact_text`]
    /// to ensure tokens never leak into logs or crash reports.
    pub fn collect_secret_strings(&self) -> Vec<&str> {
        let mut secrets = Vec::with_capacity(self.accounts.len() * 3);
        for account in &self.accounts {
            let access = account.access_token.expose_secret();
            if !access.is_empty() {
                secrets.push(access);
            }
            if let Some(refresh) = account.refresh_token.as_ref() {
                let val = refresh.expose_secret();
                if !val.is_empty() {
                    secrets.push(val);
                }
            }
            if let Some(client) = account.client_token.as_ref() {
                let val = client.expose_secret();
                if !val.is_empty() {
                    secrets.push(val);
                }
            }
        }
        secrets
    }

    fn find_account_index(&self, id_or_name: &str) -> Option<usize> {
        let query = id_or_name.trim();
        if query.is_empty() {
            return None;
        }

        // 1. Match by AccountId UUID first.
        if let Ok(parsed_uuid) = Uuid::parse_str(query) {
            if let Some(idx) = self
                .accounts
                .iter()
                .position(|a| a.id.as_uuid() == parsed_uuid)
            {
                return Some(idx);
            }
        }

        // 2. Match by exact case-sensitive username.
        if let Some(idx) = self.accounts.iter().position(|a| a.username == query) {
            return Some(idx);
        }

        // 3. Match by case-insensitive username.
        if let Some(idx) = self
            .accounts
            .iter()
            .position(|a| a.username.eq_ignore_ascii_case(query))
        {
            return Some(idx);
        }

        // 4. Fallback: match by player Minecraft UUID.
        if let Ok(parsed_uuid) = Uuid::parse_str(query) {
            if let Some(idx) = self.accounts.iter().position(|a| a.uuid == parsed_uuid) {
                return Some(idx);
            }
        }

        None
    }

    fn normalize_active_selection(&mut self) {
        if self.accounts.is_empty() {
            self.active_account_id = None;
            return;
        }

        let valid_active = self
            .active_account_id
            .is_some_and(|id| self.accounts.iter().any(|a| a.id == id));

        if !valid_active {
            self.active_account_id = self.accounts.first().map(|a| a.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use mq_core::{redact_text, SecretString};

    use super::*;
    use crate::models::AccountKind;

    fn sample_profile(
        kind: AccountKind,
        username: &str,
        access: &str,
        refresh: Option<&str>,
        client: Option<&str>,
    ) -> AccountProfile {
        AccountProfile {
            id: AccountId::new_v4(),
            kind,
            username: username.to_owned(),
            uuid: Uuid::new_v4(),
            access_token: SecretString::new(access),
            refresh_token: refresh.map(SecretString::new),
            client_token: client.map(SecretString::new),
            expires_at: None,
            skin_url: None,
            avatar_head: None,
        }
    }

    #[test]
    fn save_and_load_enforces_strict_0600_permissions_and_redacts_debug() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let paths = MqPaths::with_root(tmp.path());

        let mut store = AccountStore::load_or_default(&paths).expect("load empty default");
        assert!(store.is_empty());
        assert!(store.active_account().is_none());

        let offline_acc = sample_profile(
            AccountKind::Offline,
            "OfflineBuilder",
            "offline_session_tok_1111",
            None,
            None,
        );
        let ely_acc = sample_profile(
            AccountKind::ElyBy,
            "ElyPlayer",
            "ely_access_secret_2222",
            None,
            Some("ely_client_secret_3333"),
        );
        let msa_acc = sample_profile(
            AccountKind::Microsoft,
            "MsaExplorer",
            "msa_access_secret_4444",
            Some("msa_refresh_secret_5555"),
            None,
        );

        let offline_id = offline_acc.id;
        let ely_id = ely_acc.id;
        let msa_id = msa_acc.id;

        // First account becomes active even when make_active = false.
        store.upsert_account(offline_acc, false);
        assert_eq!(store.active_account().map(|a| a.id), Some(offline_id));

        // Second account with make_active = false preserves first as active.
        store.upsert_account(ely_acc, false);
        assert_eq!(store.active_account().map(|a| a.id), Some(offline_id));

        // Third account with make_active = true switches active selection.
        store.upsert_account(msa_acc, true);
        assert_eq!(store.active_account().map(|a| a.id), Some(msa_id));
        assert_eq!(store.list_accounts().len(), 3);

        // Confirm format!("{:?}", store) NEVER leaks raw token strings.
        let dbg_str = format!("{store:?}");
        let pretty_dbg_str = format!("{store:#?}");
        for forbidden in [
            "offline_session_tok_1111",
            "ely_access_secret_2222",
            "ely_client_secret_3333",
            "msa_access_secret_4444",
            "msa_refresh_secret_5555",
        ] {
            assert!(
                !dbg_str.contains(forbidden),
                "Debug output leaked secret {forbidden}"
            );
            assert!(
                !pretty_dbg_str.contains(forbidden),
                "Pretty Debug output leaked secret {forbidden}"
            );
        }
        assert!(dbg_str.contains("[REDACTED]"));

        // Verify collect_secret_strings works with mq_core::redact_text.
        let collected = store.collect_secret_strings();
        assert_eq!(collected.len(), 5);
        let raw_log = "Launching --accessToken msa_access_secret_4444 --refresh msa_refresh_secret_5555 --client ely_client_secret_3333";
        let scrubbed_log = redact_text(raw_log, &collected);
        assert_eq!(
            scrubbed_log,
            "Launching --accessToken [REDACTED] --refresh [REDACTED] --client [REDACTED]"
        );

        // Save to <temp>/accounts.json and verify exact Unix mode 0o600.
        store.save(&paths).expect("save accounts.json");
        let accounts_path = paths.accounts_file();
        assert!(accounts_path.is_file());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&accounts_path)
                .expect("stat accounts.json")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "accounts.json must have exact 0o600 mode");
        }

        // Reload and verify roundtrip + 1-click switching by username & AccountId.
        let mut reloaded = AccountStore::load_or_default(&paths).expect("reload accounts.json");
        assert_eq!(reloaded, store);
        assert_eq!(
            reloaded.active_account().map(|a| a.username.as_str()),
            Some("MsaExplorer")
        );

        // 1-click switch by case-insensitive username.
        let selected = reloaded
            .select_account("elyplayer")
            .expect("select by case-insensitive username");
        assert_eq!(selected.id, ely_id);
        assert_eq!(reloaded.active_account().map(|a| a.id), Some(ely_id));

        // 1-click switch by AccountId UUID string.
        let selected_by_id = reloaded
            .select_account(&offline_id.to_string())
            .expect("select by AccountId");
        assert_eq!(selected_by_id.username, "OfflineBuilder");
        assert_eq!(reloaded.active_account().map(|a| a.id), Some(offline_id));

        // Selecting a non-existent account returns an error without corrupting active selection.
        assert!(reloaded.select_account("NonExistentPlayer").is_err());
        assert_eq!(reloaded.active_account().map(|a| a.id), Some(offline_id));

        // Upserting existing (kind, username) updates in place without duplicating.
        let updated_offline = sample_profile(
            AccountKind::Offline,
            "offlinebuilder",
            "offline_session_tok_updated_9999",
            None,
            None,
        );
        let updated_offline_id = updated_offline.id;
        reloaded.upsert_account(updated_offline, false);
        assert_eq!(reloaded.list_accounts().len(), 3);
        assert_eq!(
            reloaded.active_account().map(|a| a.id),
            Some(updated_offline_id)
        );

        // Removing the active account advances active_account_id to the next available account.
        assert!(reloaded.remove_account("OfflineBuilder"));
        assert_eq!(reloaded.list_accounts().len(), 2);
        assert_eq!(reloaded.active_account().map(|a| a.id), Some(ely_id));

        assert!(!reloaded.remove_account("OfflineBuilder"));
    }
}
