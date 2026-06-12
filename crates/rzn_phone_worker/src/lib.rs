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

    if !state.persistence_enabled().await {
        state.shutdown_spawned_appium().await;
    }
    Ok(())
}
