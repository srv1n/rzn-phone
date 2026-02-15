use std::time::{SystemTime, UNIX_EPOCH};

use std::collections::HashMap;
use tokio::process::Child;
use tokio::sync::Mutex;

use crate::ui_compact::TargetLocator;

#[derive(Debug, Clone)]
pub enum AppiumSource {
    Env,
    Spawned,
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

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionState {
    pub session_id: String,
    pub kind: String,
    pub udid: String,
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
            appium_source: guard.appium_source.clone(),
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
        let mut guard = self.inner.lock().await;
        guard.appium_base_url = Some(base_url);
        guard.appium_source = Some(source);
        guard.appium_pid = pid;
        guard.appium_child = child;
    }

    pub async fn clear_appium_metadata(&self) {
        let mut guard = self.inner.lock().await;
        guard.appium_base_url = None;
        guard.appium_source = None;
        guard.appium_pid = None;
    }

    pub async fn set_session(
        &self,
        session_id: String,
        kind: String,
        udid: String,
        wda_local_port: Option<u16>,
    ) {
        let mut guard = self.inner.lock().await;
        guard.last_udid = Some(udid.clone());
        guard.last_wda_local_port = wda_local_port;
        guard.session = Some(SessionState {
            session_id,
            kind,
            udid,
            wda_local_port,
            created_at_epoch: now_epoch(),
        });
        guard.compact_observation = None;
    }

    pub async fn active_session(&self) -> Option<SessionState> {
        self.inner.lock().await.session.clone()
    }

    pub async fn clear_session(&self) {
        let mut guard = self.inner.lock().await;
        guard.session = None;
        guard.compact_observation = None;
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
        let child_to_kill = {
            let mut guard = self.inner.lock().await;
            let child = guard.appium_child.take();
            guard.appium_base_url = None;
            guard.appium_source = None;
            guard.appium_pid = None;
            guard.session = None;
            guard.compact_observation = None;
            guard.last_udid = None;
            guard.last_wda_local_port = None;
            child
        };

        if let Some(mut child) = child_to_kill {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
