use serde_json::{json, Value};

use super::registry::tool;

#[cfg(test)]
pub(crate) const TOOL_NAMES: &[&str] = &[
    "rzn.worker.health",
    "rzn.worker.shutdown",
    "ios.env.doctor",
    "ios.device.list",
    "ios.device.status",
    "ios.appium.ensure",
    "ios.session.create",
    "ios.session.delete",
    "ios.wda.shutdown",
    "ios.session.info",
];

pub(crate) fn definitions() -> Vec<Value> {
    vec![
        tool(
            "rzn.worker.health",
            "Health check and runtime status for the rzn-phone worker runtime.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
        tool(
            "rzn.worker.shutdown",
            "Gracefully close active session and optionally stop spawned Appium.",
            json!({
                "type": "object",
                "properties": {
                    "stopAppium": { "type": "boolean", "default": true },
                    "shutdownWDA": { "type": "boolean", "default": true },
                    "backgroundApp": { "type": "boolean", "default": false, "description": "Press Home before ending session (best-effort)." },
                    "lockDevice": { "type": "boolean", "default": false, "description": "Lock device before ending session (best-effort)." }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.env.doctor",
            "Check local environment prerequisites (Xcode, xctrace, Node, Appium, xcuitest driver).",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
        tool(
            "ios.device.list",
            "List available iOS devices from xcrun xctrace.",
            json!({
                "type": "object",
                "properties": {
                    "includeSimulators": { "type": "boolean", "default": false }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.device.status",
            "Probe a specific iOS device UDID through xcrun xctrace and report whether it is available, offline, or missing.",
            json!({
                "type": "object",
                "properties": {
                    "udid": { "type": "string" }
                },
                "required": ["udid"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.appium.ensure",
            "Ensure a working Appium endpoint. Prefers RZN_IOS_APPIUM_URL, falls back to spawning Appium.",
            json!({
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "minimum": 1, "maximum": 65535, "default": 4723 },
                    "logLevel": { "type": "string", "default": "warn" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.session.create",
            "Create an iOS automation session on a real device (Safari web or native app).",
            json!({
                "type": "object",
                "properties": {
                    "udid": { "type": "string" },
                    "kind": { "type": "string", "enum": ["safari_web", "native_app"], "default": "safari_web" },
                    "bundleId": { "type": "string", "description": "Required when kind=native_app (e.g. com.reddit.Reddit)." },
                    "noReset": { "type": "boolean", "default": true },
                    "newCommandTimeoutSec": { "type": "integer", "default": 60 },
                    "sessionCreateTimeoutMs": { "type": "integer", "default": 600000 },
                    "wdaLocalPort": { "type": "integer", "minimum": 1, "maximum": 65535 },
                    "wdaLaunchTimeoutMs": { "type": "integer", "default": 240000 },
                    "wdaConnectionTimeoutMs": { "type": "integer", "default": 120000 },
                    "replaceExisting": { "type": "boolean", "default": true },
                    "showXcodeLog": { "type": "boolean", "default": false },
                    "allowProvisioningUpdates": { "type": "boolean", "default": false },
                    "allowProvisioningDeviceRegistration": { "type": "boolean", "default": false },
                    "language": { "type": "string" },
                    "locale": { "type": "string" },
                    "signing": {
                        "type": "object",
                        "properties": {
                            "xcodeOrgId": { "type": "string" },
                            "xcodeSigningId": { "type": "string" },
                            "updatedWDABundleId": { "type": "string" }
                        },
                        "additionalProperties": false
                    }
                },
                "required": ["udid"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.session.delete",
            "Delete a WebDriver session.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "stopAppium": { "type": "boolean", "default": false },
                    "shutdownWDA": { "type": "boolean", "default": true }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.wda.shutdown",
            "Best-effort shutdown of WebDriverAgent/XCTest (clears the 'Automation Running' overlay on-device).",
            json!({
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "minimum": 1, "maximum": 65535, "default": 8100 }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.session.info",
            "Return active session metadata and Appium endpoint details.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
    ]
}
