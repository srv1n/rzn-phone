pub mod appium;
pub mod errors;
pub mod jsonrpc;
pub mod mcp;
pub mod state;
pub mod tool_policy;
pub mod tools;
pub mod ui_compact;
pub mod webdriver;
pub mod workflow_failure_report;
pub mod workflows;
pub mod xctrace;

use serde_json::json;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::jsonrpc::{
    build_error_response, build_result_response, parse_incoming_line, IncomingMessage,
};
use crate::state::AppState;

pub async fn run_worker_stdio() -> anyhow::Result<()> {
    let state = AppState::new();

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let mut stdout = io::stdout();

    while let Some(line) = reader.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match parse_incoming_line(line) {
            Ok(IncomingMessage::Request(request)) => {
                let response = match mcp::handle_request(&state, &request.method, request.params)
                    .await
                {
                    Ok(result) => build_result_response(request.id, result),
                    Err(err) => build_error_response(request.id, err.code, &err.message, err.data),
                };
                let payload = serde_json::to_string(&response)?;
                stdout.write_all(payload.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
            Ok(IncomingMessage::Notification(notification)) => {
                if notification.method == "shutdown" {
                    state.shutdown_spawned_appium().await;
                }
                let _ = notification.params;
            }
            Err(err) => {
                let response = build_error_response(
                    json!(null),
                    -32700,
                    "parse error",
                    Some(json!({ "error": err })),
                );
                let payload = serde_json::to_string(&response)?;
                stdout.write_all(payload.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
        }
    }

    cleanup_worker_on_stdin_close(&state).await;
    Ok(())
}

async fn cleanup_worker_on_stdin_close(state: &AppState) {
    if state.persistence_enabled().await {
        return;
    }

    let _ = crate::tools::handle_tool_call(
        state,
        "rzn.worker.shutdown",
        json!({
            "commit": true,
            "stopAppium": true,
            "shutdownWDA": true,
            "backgroundApp": false,
            "lockDevice": false
        }),
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppiumSource, TEST_ENV_LOCK};
    use httpmock::{
        Method::{DELETE, GET},
        MockServer,
    };
    use std::ffi::OsString;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn remove(key: &'static str) -> Self {
            let previous = std::env::var_os(key);
            std::env::remove_var(key);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    #[tokio::test]
    async fn stdin_close_cleanup_deletes_active_webdriver_session() {
        let _guard = TEST_ENV_LOCK.lock().await;
        let _persist_guard = EnvVarGuard::remove("RZN_IOS_PERSIST_RUNTIME");
        let _state_file_guard = EnvVarGuard::remove("RZN_IOS_RUNTIME_STATE_FILE");
        let server = MockServer::start_async().await;
        let delete_mock = server
            .mock_async(|when, then| {
                when.method(DELETE).path("/session/sess-1");
                then.status(200).json_body(json!({"value": null}));
            })
            .await;
        let wda_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/wda/shutdown");
                then.status(200).body("ok");
            })
            .await;
        let state = AppState::new();
        state
            .set_appium(server.url(""), AppiumSource::Env, None, None)
            .await;
        state
            .set_session(
                "sess-1".to_string(),
                "native_app".to_string(),
                "TEST-UDID".to_string(),
                Some("com.example.app".to_string()),
                Some(server.port()),
            )
            .await;

        cleanup_worker_on_stdin_close(&state).await;

        delete_mock.assert_async().await;
        wda_mock.assert_async().await;
        assert!(state.active_session().await.is_none());
        assert!(state.appium_base_url().await.is_none());
    }
}
