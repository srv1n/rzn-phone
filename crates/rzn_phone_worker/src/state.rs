use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::ui_compact::TargetLocator;

#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: once_cell::sync::Lazy<tokio::sync::Mutex<()>> =
    once_cell::sync::Lazy::new(|| tokio::sync::Mutex::new(()));

#[cfg(test)]
type PersistenceWriteHook = std::sync::Arc<dyn Fn() + Send + Sync>;

#[cfg(test)]
static PERSISTENCE_WRITE_HOOK: once_cell::sync::Lazy<
    std::sync::Mutex<Option<PersistenceWriteHook>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

const DEFAULT_RUNTIME_CACHE_TTL_SECS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppiumSource {
    Env,
    Spawned,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistedRuntimeState {
    appium_base_url: Option<String>,
    appium_source: Option<String>,
    appium_pid: Option<u32>,
    session: Option<SessionState>,
    last_udid: Option<String>,
    last_wda_local_port: Option<u16>,
    #[serde(default)]
    last_used_epoch_ms: u64,
}

#[derive(Debug)]
struct RuntimeState {
    appium_base_url: Option<String>,
    appium_source: Option<AppiumSource>,
    appium_pid: Option<u32>,
    appium_child: Option<Child>,
    session: Option<SessionState>,
    compact_observation: Option<CompactObservation>,
    last_udid: Option<String>,
    last_wda_local_port: Option<u16>,
}

#[derive(Debug)]
enum RuntimePersistenceUpdate {
    Clear,
    Save(Box<PersistedRuntimeState>),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub kind: String,
    pub udid: String,
    #[serde(default)]
    pub bundle_id: Option<String>,
    pub wda_local_port: Option<u16>,
    pub created_at_epoch: u64,
}

#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub appium_base_url: Option<String>,
    pub appium_source: Option<AppiumSource>,
    pub appium_pid: Option<u32>,
    pub session: Option<SessionState>,
}

#[derive(Debug, Clone)]
pub struct CompactObservation {
    pub snapshot_id: String,
    pub session_id: String,
    pub created_at_epoch: u64,
    pub targets: HashMap<String, TargetLocator>,
}

#[derive(Debug)]
pub struct AppState {
    inner: Mutex<RuntimeState>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(RuntimeState {
                appium_base_url: None,
                appium_source: None,
                appium_pid: None,
                appium_child: None,
                session: None,
                compact_observation: None,
                last_udid: None,
                last_wda_local_port: None,
            }),
        }
    }

    pub async fn snapshot(&self) -> StateSnapshot {
        let guard = self.inner.lock().await;
        StateSnapshot {
            appium_base_url: guard.appium_base_url.clone(),
            appium_source: guard.appium_source,
            appium_pid: guard.appium_pid,
            session: guard.session.clone(),
        }
    }

    pub async fn appium_base_url(&self) -> Option<String> {
        self.inner.lock().await.appium_base_url.clone()
    }

    pub async fn set_appium(
        &self,
        base_url: String,
        source: AppiumSource,
        pid: Option<u32>,
        child: Option<Child>,
    ) {
        let persistence = {
            let mut guard = self.inner.lock().await;
            guard.appium_base_url = Some(base_url);
            guard.appium_source = Some(source);
            guard.appium_pid = pid;
            guard.appium_child = child;
            runtime_persistence_update(&guard)
        };
        persist_runtime_state(persistence).await;
    }

    pub async fn refresh_appium_metadata(
        &self,
        base_url: String,
        source: AppiumSource,
        pid: Option<u32>,
    ) {
        let persistence = {
            let mut guard = self.inner.lock().await;
            let preserve_spawned_child = matches!(source, AppiumSource::Spawned)
                && matches!(guard.appium_source, Some(AppiumSource::Spawned))
                && guard
                    .appium_base_url
                    .as_deref()
                    .is_some_and(|existing| same_appium_server(existing, &base_url));

            guard.appium_base_url = Some(base_url);
            guard.appium_source = Some(source);
            guard.appium_pid = pid;
            if !preserve_spawned_child {
                guard.appium_child = None;
            }
            runtime_persistence_update(&guard)
        };
        persist_runtime_state(persistence).await;
    }

    pub async fn clear_appium_metadata(&self) {
        let persistence = {
            let mut guard = self.inner.lock().await;
            guard.appium_base_url = None;
            guard.appium_source = None;
            guard.appium_pid = None;
            guard.appium_child = None;
            runtime_persistence_update(&guard)
        };
        persist_runtime_state(persistence).await;
    }

    pub async fn set_session(
        &self,
        session_id: String,
        kind: String,
        udid: String,
        bundle_id: Option<String>,
        wda_local_port: Option<u16>,
    ) {
        let persistence = {
            let mut guard = self.inner.lock().await;
            guard.last_udid = Some(udid.clone());
            guard.last_wda_local_port = wda_local_port;
            guard.session = Some(SessionState {
                session_id,
                kind,
                udid,
                bundle_id,
                wda_local_port,
                created_at_epoch: now_epoch(),
            });
            guard.compact_observation = None;
            runtime_persistence_update(&guard)
        };
        persist_runtime_state(persistence).await;
    }

    pub async fn active_session(&self) -> Option<SessionState> {
        self.inner.lock().await.session.clone()
    }

    pub async fn clear_session(&self) {
        let persistence = {
            let mut guard = self.inner.lock().await;
            guard.session = None;
            guard.compact_observation = None;
            runtime_persistence_update(&guard)
        };
        persist_runtime_state(persistence).await;
    }

    pub async fn last_udid(&self) -> Option<String> {
        self.inner.lock().await.last_udid.clone()
    }

    pub async fn last_wda_local_port(&self) -> Option<u16> {
        self.inner.lock().await.last_wda_local_port
    }

    pub async fn set_compact_observation(
        &self,
        snapshot_id: String,
        session_id: String,
        targets: HashMap<String, TargetLocator>,
    ) {
        let mut guard = self.inner.lock().await;
        guard.compact_observation = Some(CompactObservation {
            snapshot_id,
            session_id,
            created_at_epoch: now_epoch(),
            targets,
        });
    }

    pub async fn resolve_compact_target(
        &self,
        snapshot_id: Option<&str>,
        encoded_id: &str,
    ) -> Option<TargetLocator> {
        let guard = self.inner.lock().await;
        let obs = guard.compact_observation.as_ref()?;
        if let Some(want) = snapshot_id {
            if want != obs.snapshot_id {
                return None;
            }
        }
        obs.targets.get(encoded_id).cloned()
    }

    pub async fn compact_snapshot_id(&self) -> Option<String> {
        self.inner
            .lock()
            .await
            .compact_observation
            .as_ref()
            .map(|obs| obs.snapshot_id.clone())
    }

    pub async fn shutdown_spawned_appium(&self) {
        let (child_to_kill, pid_to_kill, persistence) = {
            let mut guard = self.inner.lock().await;
            let child = guard.appium_child.take();
            let pid = guard.appium_pid;
            guard.appium_base_url = None;
            guard.appium_source = None;
            guard.appium_pid = None;
            guard.session = None;
            guard.compact_observation = None;
            guard.last_udid = None;
            guard.last_wda_local_port = None;
            (child, pid, runtime_persistence_update(&guard))
        };
        persist_runtime_state(persistence).await;

        if let Some(mut child) = child_to_kill {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return;
        }

        if let Some(pid) = pid_to_kill {
            if !should_kill_persisted_appium_pid(pid).await {
                return;
            }
            let pid = pid.to_string();
            let _ = Command::new("kill").args(["-TERM", &pid]).status().await;
            sleep(Duration::from_millis(300)).await;
            let _ = Command::new("kill").args(["-KILL", &pid]).status().await;
        }
    }

    pub async fn restore_persisted_runtime(&self) -> bool {
        let Some(restored) = load_persisted_runtime_state() else {
            return false;
        };
        if persisted_runtime_is_stale(&restored) {
            clear_persisted_runtime_file();
            return false;
        }

        let mut guard = self.inner.lock().await;
        let mut changed = false;

        if guard.appium_base_url.is_none() {
            guard.appium_base_url = restored.appium_base_url.clone();
            guard.appium_source = restored
                .appium_source
                .as_deref()
                .and_then(parse_appium_source);
            guard.appium_pid = restored.appium_pid;
            changed = changed || guard.appium_base_url.is_some();
        }

        if guard.session.is_none() {
            guard.session = restored.session.clone();
            changed = changed || guard.session.is_some();
        }

        if guard.last_udid.is_none() {
            guard.last_udid = restored.last_udid.clone();
        }

        if guard.last_wda_local_port.is_none() {
            guard.last_wda_local_port = restored.last_wda_local_port;
        }

        changed
    }

    pub async fn persistence_enabled(&self) -> bool {
        persistence_enabled()
    }

    pub async fn touch_runtime(&self) {
        let persistence = {
            let guard = self.inner.lock().await;
            runtime_persistence_update(&guard)
        };
        persist_runtime_state(persistence).await;
    }

    #[cfg(test)]
    pub async fn appium_child_id(&self) -> Option<u32> {
        self.inner
            .lock()
            .await
            .appium_child
            .as_ref()
            .and_then(Child::id)
    }

    #[cfg(test)]
    pub async fn has_appium_child(&self) -> bool {
        self.inner.lock().await.appium_child.is_some()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn persistence_enabled() -> bool {
    env::var("RZN_IOS_PERSIST_RUNTIME")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn persistence_file_path() -> Option<PathBuf> {
    if !persistence_enabled() {
        return None;
    }

    if let Ok(explicit) = env::var("RZN_IOS_RUNTIME_STATE_FILE") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    Some(crate::config::state_dir().join("runtime-state.json"))
}

fn runtime_persistence_update(state: &RuntimeState) -> RuntimePersistenceUpdate {
    if state.appium_base_url.is_none() && state.session.is_none() {
        return RuntimePersistenceUpdate::Clear;
    }

    RuntimePersistenceUpdate::Save(Box::new(PersistedRuntimeState {
        appium_base_url: state.appium_base_url.clone(),
        appium_source: state.appium_source.as_ref().map(appium_source_name),
        appium_pid: state.appium_pid,
        session: state.session.clone(),
        last_udid: state.last_udid.clone(),
        last_wda_local_port: state.last_wda_local_port,
        last_used_epoch_ms: now_epoch_ms(),
    }))
}

async fn persist_runtime_state(update: RuntimePersistenceUpdate) {
    let Some(path) = persistence_file_path() else {
        return;
    };

    let _ = tokio::task::spawn_blocking(move || persist_runtime_state_blocking(path, update)).await;
}

fn persist_runtime_state_blocking(path: PathBuf, update: RuntimePersistenceUpdate) {
    match update {
        RuntimePersistenceUpdate::Clear => {
            let _ = fs::remove_file(path);
        }
        RuntimePersistenceUpdate::Save(payload) => write_persisted_runtime_state(path, *payload),
    }
}

fn write_persisted_runtime_state(path: PathBuf, payload: PersistedRuntimeState) {
    if let Some(parent) = path.parent() {
        let _ = create_private_dir(parent);
    }

    let Ok(json) = serde_json::to_vec_pretty(&payload) else {
        return;
    };

    run_persistence_write_hook();

    let tmp_path = path.with_extension("tmp");
    if write_private_file(&tmp_path, &json).is_ok() {
        let _ = fs::rename(&tmp_path, &path);
        let _ = restrict_file_permissions(&path);
    }
}

#[cfg(test)]
fn set_persistence_write_hook(hook: Option<PersistenceWriteHook>) {
    *PERSISTENCE_WRITE_HOOK
        .lock()
        .expect("persistence hook lock") = hook;
}

#[cfg(test)]
fn run_persistence_write_hook() {
    let hook = PERSISTENCE_WRITE_HOOK
        .lock()
        .expect("persistence hook lock")
        .clone();
    if let Some(hook) = hook {
        hook();
    }
}

#[cfg(not(test))]
fn run_persistence_write_hook() {}

fn load_persisted_runtime_state() -> Option<PersistedRuntimeState> {
    let path = persistence_file_path()?;
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn persisted_runtime_is_stale(state: &PersistedRuntimeState) -> bool {
    let ttl_ms = runtime_cache_ttl_ms();
    ttl_ms != 0 && now_epoch_ms().saturating_sub(state.last_used_epoch_ms) > ttl_ms
}

fn runtime_cache_ttl_ms() -> u64 {
    env::var("RZN_IOS_RUNTIME_CACHE_TTL_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_RUNTIME_CACHE_TTL_SECS)
        .saturating_mul(1000)
}

fn clear_persisted_runtime_file() {
    let Some(path) = persistence_file_path() else {
        return;
    };
    let _ = fs::remove_file(path);
}

fn appium_source_name(source: &AppiumSource) -> String {
    match source {
        AppiumSource::Env => "env".to_string(),
        AppiumSource::Spawned => "spawned".to_string(),
    }
}

fn parse_appium_source(value: &str) -> Option<AppiumSource> {
    match value.trim() {
        "env" => Some(AppiumSource::Env),
        "spawned" => Some(AppiumSource::Spawned),
        _ => None,
    }
}

fn same_appium_server(left: &str, right: &str) -> bool {
    appium_server_key(left) == appium_server_key(right)
}

fn appium_server_key(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)?;
    restrict_dir_permissions(path)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        restrict_file_permissions(path)?;
        Ok(())
    }

    #[cfg(not(unix))]
    {
        fs::write(path, bytes)
    }
}

fn restrict_dir_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn restrict_file_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

async fn should_kill_persisted_appium_pid(pid: u32) -> bool {
    if !env_flag("RZN_IOS_ALLOW_PERSISTED_APPIUM_PID_KILL") {
        return false;
    }
    process_command_contains_appium(pid).await
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

async fn process_command_contains_appium(pid: u32) -> bool {
    let Ok(output) = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .await
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let command = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    command.contains("appium")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Condvar, Mutex as StdMutex};

    #[tokio::test]
    async fn persisted_runtime_round_trips_across_app_state_instances() {
        let _guard = TEST_ENV_LOCK.lock().await;
        let root = env::temp_dir().join(format!(
            "rzn-phone-state-{}-{}",
            std::process::id(),
            now_epoch_ms()
        ));
        let state_path = root.join("runtime-state.json");
        let _ = fs::remove_dir_all(&root);
        env::set_var("RZN_IOS_PERSIST_RUNTIME", "1");
        env::set_var("RZN_IOS_RUNTIME_STATE_FILE", &state_path);

        let state = AppState::new();
        state
            .set_appium(
                "http://127.0.0.1:4723".to_string(),
                AppiumSource::Spawned,
                Some(4242),
                None,
            )
            .await;
        state
            .set_session(
                "session-123".to_string(),
                "safari_web".to_string(),
                "udid-1".to_string(),
                None,
                Some(8100),
            )
            .await;

        let restored = AppState::new();
        assert!(restored.restore_persisted_runtime().await);
        let snapshot = restored.snapshot().await;
        let session = snapshot.session.expect("persisted session");
        assert_eq!(
            snapshot.appium_base_url.as_deref(),
            Some("http://127.0.0.1:4723")
        );
        assert_eq!(snapshot.appium_pid, Some(4242));
        assert_eq!(session.session_id, "session-123");
        assert_eq!(session.kind, "safari_web");
        assert_eq!(session.udid, "udid-1");
        assert_eq!(session.bundle_id, None);
        assert_eq!(session.wda_local_port, Some(8100));

        let _ = fs::remove_file(&state_path);
        let _ = fs::remove_dir(&root);
        env::remove_var("RZN_IOS_PERSIST_RUNTIME");
        env::remove_var("RZN_IOS_RUNTIME_STATE_FILE");
    }

    #[tokio::test]
    async fn stale_persisted_runtime_is_not_restored() {
        let _guard = TEST_ENV_LOCK.lock().await;
        let root = env::temp_dir().join(format!(
            "rzn-phone-stale-state-{}-{}",
            std::process::id(),
            now_epoch_ms()
        ));
        let state_path = root.join("runtime-state.json");
        let old_persist = env::var_os("RZN_IOS_PERSIST_RUNTIME");
        let old_state_file = env::var_os("RZN_IOS_RUNTIME_STATE_FILE");
        let old_ttl = env::var_os("RZN_IOS_RUNTIME_CACHE_TTL_SECS");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("state dir");
        env::set_var("RZN_IOS_PERSIST_RUNTIME", "1");
        env::set_var("RZN_IOS_RUNTIME_STATE_FILE", &state_path);
        env::set_var("RZN_IOS_RUNTIME_CACHE_TTL_SECS", "1");

        let stale = PersistedRuntimeState {
            appium_base_url: Some("http://127.0.0.1:4723".to_string()),
            appium_source: Some("spawned".to_string()),
            appium_pid: Some(4242),
            session: Some(SessionState {
                session_id: "session-stale".to_string(),
                kind: "safari_web".to_string(),
                udid: "udid-1".to_string(),
                bundle_id: None,
                wda_local_port: Some(8100),
                created_at_epoch: now_epoch().saturating_sub(60),
            }),
            last_udid: Some("udid-1".to_string()),
            last_wda_local_port: Some(8100),
            last_used_epoch_ms: now_epoch_ms().saturating_sub(10_000),
        };
        fs::write(&state_path, serde_json::to_vec(&stale).unwrap()).expect("state file");

        let restored = AppState::new();
        assert!(!restored.restore_persisted_runtime().await);
        let snapshot = restored.snapshot().await;
        assert!(snapshot.appium_base_url.is_none());
        assert!(snapshot.session.is_none());
        assert!(
            !state_path.exists(),
            "stale runtime cache should be discarded"
        );

        let _ = fs::remove_dir_all(&root);
        restore_env("RZN_IOS_PERSIST_RUNTIME", old_persist);
        restore_env("RZN_IOS_RUNTIME_STATE_FILE", old_state_file);
        restore_env("RZN_IOS_RUNTIME_CACHE_TTL_SECS", old_ttl);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn persistence_write_does_not_hold_state_mutex() {
        let _guard = TEST_ENV_LOCK.lock().await;
        let root = env::temp_dir().join(format!(
            "rzn-phone-nonblocking-state-{}-{}",
            std::process::id(),
            now_epoch_ms()
        ));
        let state_path = root.join("runtime-state.json");
        let old_persist = env::var_os("RZN_IOS_PERSIST_RUNTIME");
        let old_state_file = env::var_os("RZN_IOS_RUNTIME_STATE_FILE");
        let _ = fs::remove_dir_all(&root);
        env::set_var("RZN_IOS_PERSIST_RUNTIME", "1");
        env::set_var("RZN_IOS_RUNTIME_STATE_FILE", &state_path);

        let entered = Arc::new((StdMutex::new(false), Condvar::new()));
        let release = Arc::new((StdMutex::new(false), Condvar::new()));
        let hook_entered = Arc::clone(&entered);
        let hook_release = Arc::clone(&release);
        set_persistence_write_hook(Some(Arc::new(move || {
            let (entered_lock, entered_cvar) = &*hook_entered;
            *entered_lock.lock().expect("entered lock") = true;
            entered_cvar.notify_all();

            let (release_lock, release_cvar) = &*hook_release;
            let mut released = release_lock.lock().expect("release lock");
            while !*released {
                released = release_cvar.wait(released).expect("release wait");
            }
        })));

        let state = Arc::new(AppState::new());
        let writer_state = Arc::clone(&state);
        let writer = tokio::spawn(async move {
            writer_state
                .set_appium(
                    "http://127.0.0.1:4723".to_string(),
                    AppiumSource::Spawned,
                    Some(4242),
                    None,
                )
                .await;
        });

        let mut saw_hook = false;
        for _ in 0..100 {
            if *entered.0.lock().expect("entered lock") {
                saw_hook = true;
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }

        let snapshot_result =
            tokio::time::timeout(Duration::from_millis(200), state.snapshot()).await;

        *release.0.lock().expect("release lock") = true;
        release.1.notify_all();
        let writer_result = tokio::time::timeout(Duration::from_secs(2), writer).await;
        set_persistence_write_hook(None);
        let _ = fs::remove_dir_all(&root);
        restore_env("RZN_IOS_PERSIST_RUNTIME", old_persist);
        restore_env("RZN_IOS_RUNTIME_STATE_FILE", old_state_file);

        assert!(saw_hook, "persistence write hook was not reached");
        writer_result
            .expect("writer should finish after hook release")
            .expect("writer task");
        let snapshot = snapshot_result.expect("snapshot should not wait for persistence write");
        assert_eq!(
            snapshot.appium_base_url.as_deref(),
            Some("http://127.0.0.1:4723")
        );
    }

    #[tokio::test]
    async fn runtime_guardrails_persisted_pid_kill_is_disabled_by_default() {
        let _guard = TEST_ENV_LOCK.lock().await;
        env::remove_var("RZN_IOS_ALLOW_PERSISTED_APPIUM_PID_KILL");

        assert!(!should_kill_persisted_appium_pid(std::process::id()).await);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn privacy_persisted_runtime_uses_private_unix_permissions() {
        let _guard = TEST_ENV_LOCK.lock().await;
        let root = env::temp_dir().join(format!(
            "rzn-phone-private-state-{}-{}",
            std::process::id(),
            now_epoch_ms()
        ));
        let state_path = root.join("runtime-state.json");
        env::set_var("RZN_IOS_PERSIST_RUNTIME", "1");
        env::set_var("RZN_IOS_RUNTIME_STATE_FILE", &state_path);

        let state = AppState::new();
        state
            .set_appium(
                "http://127.0.0.1:4723".to_string(),
                AppiumSource::Spawned,
                None,
                None,
            )
            .await;

        let dir_mode = fs::metadata(&root).unwrap().permissions().mode() & 0o777;
        let file_mode = fs::metadata(&state_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700);
        assert_eq!(file_mode, 0o600);

        let _ = fs::remove_file(&state_path);
        let _ = fs::remove_dir(&root);
        env::remove_var("RZN_IOS_PERSIST_RUNTIME");
        env::remove_var("RZN_IOS_RUNTIME_STATE_FILE");
    }

    fn restore_env(name: &str, value: Option<std::ffi::OsString>) {
        if let Some(value) = value {
            env::set_var(name, value);
        } else {
            env::remove_var(name);
        }
    }
}
