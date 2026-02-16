use anyhow::{anyhow, bail, Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::str;
use std::time::Duration;
use tokio::process::Command;

use crate::appium::{ensure_appium, probe_webdriver_base, EnsureOptions};
use crate::errors::{ToolCallError, ToolErrorCode};
use crate::state::{AppState, AppiumSource};
use crate::ui_compact::{build_compact_snapshot, locator_to_json, NodeFilter};
use crate::webdriver::{SessionCreateRequest, WebDriverClient};
use crate::workflows;
use crate::xctrace;

const DEFAULT_WDA_LOCAL_PORT: u16 = 8100;

pub fn list_tool_definitions() -> Vec<Value> {
    vec![
        tool(
            "rzn.worker.health",
            "Health check and runtime status for ios-tools worker.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
        tool(
            "rzn.worker.shutdown",
            "Gracefully close active session and optionally stop spawned Appium.",
            json!({
                "type": "object",
                "properties": {
                    "stopAppium": { "type": "boolean", "default": true },
                    "shutdownWDA": { "type": "boolean", "default": true }
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
        tool(
            "ios.ui.source",
            "Get the current UI hierarchy source XML for the active session (native or web).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.ui.screenshot",
            "Capture a screenshot from the active session (native or web).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.ui.observe_compact",
            "Return a compact, LLM-friendly UI snapshot (native apps only in MVP). Encoded ids can be used with ios.action.* tools.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "nodeFilter": { "type": "string", "enum": ["interactive", "all"], "default": "interactive" },
                    "maxNodes": { "type": "integer", "default": 140 }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.ui.extract_rows",
            "Extract ordered rows from a UI source XML using generic selectors and splitting rules.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "source": { "type": "string" },
                    "row": {
                        "type": "object",
                        "properties": {
                            "type": { "type": "string", "default": "XCUIElementTypeCell" },
                            "name": { "type": "string" },
                            "namePrefix": { "type": "string" },
                            "ancestorName": { "type": "string" },
                            "ancestorType": { "type": "string" }
                        },
                        "additionalProperties": false
                    },
                    "primary": {
                        "type": "object",
                        "properties": {
                            "type": { "type": "string", "default": "XCUIElementTypeButton" },
                            "attr": { "type": "string", "enum": ["label", "name", "value"], "default": "label" },
                            "pick": { "type": "string", "enum": ["first", "longest"], "default": "longest" }
                        },
                        "additionalProperties": false
                    },
                    "tag": {
                        "type": "object",
                        "properties": {
                            "namePrefix": { "type": "string" },
                            "pick": { "type": "string", "enum": ["first", "last"], "default": "last" },
                            "stripPrefix": { "type": "string" }
                        },
                        "additionalProperties": false
                    },
                    "split": {
                        "type": "object",
                        "properties": {
                            "delimiter": { "type": "string", "default": "," },
                            "ignorePrefixes": { "type": "array", "items": { "type": "string" } },
                            "fields": { "type": "array", "items": { "type": "string" } },
                            "skipMetricLike": { "type": "boolean", "default": true }
                        },
                        "additionalProperties": false
                    },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 },
                    "maxScrolls": { "type": "integer", "minimum": 0, "maximum": 50, "default": 0 },
                    "scroll": {
                        "type": "object",
                        "properties": {
                            "direction": { "type": "string", "enum": ["down", "up", "left", "right"], "default": "down" },
                            "distance": { "type": "number", "minimum": 0.1, "maximum": 0.95, "default": 0.6 },
                            "settleMs": { "type": "integer", "minimum": 0, "maximum": 10000, "default": 350 }
                        },
                        "additionalProperties": false
                    },
                    "order": { "type": "string", "enum": ["y", "x"], "default": "y" }
                },
                "required": ["row", "primary"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.target.resolve",
            "Resolve an encoded id from the latest compact snapshot into a WebDriver locator.",
            json!({
                "type": "object",
                "properties": {
                    "encodedId": { "type": "string" },
                    "snapshotId": { "type": "string" }
                },
                "required": ["encodedId"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.tap",
            "Tap a UI element by encoded id (preferred), locator (using/value), or point.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
	                    "target": {
	                        "type": "object",
	                        "properties": {
	                            "encodedId": { "type": "string" },
	                            "snapshotId": { "type": "string" },
	                            "using": { "type": "string" },
	                            "value": { "type": "string" },
	                            "index": { "type": "integer", "minimum": 0, "default": 0 },
	                            "requireUnique": { "type": "boolean", "default": false }
	                        },
	                        "additionalProperties": false
	                    },
                    "point": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "number" },
                            "y": { "type": "number" }
                        },
                        "required": ["x", "y"],
                        "additionalProperties": false
                    }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.type",
            "Type text into a UI field by encoded id (preferred) or locator (using/value).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
	                    "target": {
	                        "type": "object",
	                        "properties": {
	                            "encodedId": { "type": "string" },
	                            "snapshotId": { "type": "string" },
	                            "using": { "type": "string" },
	                            "value": { "type": "string" },
	                            "index": { "type": "integer", "minimum": 0, "default": 0 },
	                            "requireUnique": { "type": "boolean", "default": false }
	                        },
	                        "additionalProperties": false
	                    },
                    "text": { "type": "string" },
                    "clearFirst": { "type": "boolean", "default": true }
                },
                "required": ["text"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.typeahead",
            "Type a query or prefixes into a field and capture ordered typeahead suggestions (generic).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "field": {
                        "type": "object",
                        "properties": {
                            "encodedId": { "type": "string" },
                            "snapshotId": { "type": "string" },
                            "using": { "type": "string" },
                            "value": { "type": "string" },
                            "index": { "type": "integer", "minimum": 0, "default": 0 },
                            "requireUnique": { "type": "boolean", "default": false }
                        },
                        "additionalProperties": false
                    },
                    "query": { "type": "string" },
                    "prefixes": { "type": "array", "items": { "type": "string" } },
                    "limit": { "type": "integer", "default": 10, "minimum": 1, "maximum": 20 },
                    "typingMode": { "type": "string", "default": "full" },
                    "suggestionQuery": {
                        "type": "object",
                        "properties": {
                            "type": { "type": "string", "default": "XCUIElementTypeCell" },
                            "name": { "type": "string" },
                            "namePrefix": { "type": "string" },
                            "label": { "type": "string" },
                            "ancestorName": { "type": "string" },
                            "ancestorType": { "type": "string" },
                            "max": { "type": "integer", "minimum": 1, "maximum": 50 }
                        },
                        "additionalProperties": false
                    }
                },
                "required": ["field"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.wait",
            "Wait for an element to exist by encoded id (preferred) or locator (using/value).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
	                    "target": {
	                        "type": "object",
	                        "properties": {
	                            "encodedId": { "type": "string" },
	                            "snapshotId": { "type": "string" },
	                            "using": { "type": "string" },
	                            "value": { "type": "string" },
	                            "index": { "type": "integer", "minimum": 0, "default": 0 },
	                            "requireUnique": { "type": "boolean", "default": false }
	                        },
	                        "additionalProperties": false
	                    },
                    "timeoutMs": { "type": "integer", "default": 10000 }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.scroll",
            "Scroll the screen in a direction (uses touch pointer actions).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "direction": { "type": "string", "enum": ["up", "down", "left", "right"] },
                    "distance": { "type": "number", "default": 0.6 }
                },
                "required": ["direction"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.action.swipe",
            "Swipe the screen in a direction (alias of ios.action.scroll).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "direction": { "type": "string", "enum": ["up", "down", "left", "right"] },
                    "distance": { "type": "number", "default": 0.6 }
                },
                "required": ["direction"],
                "additionalProperties": false
            }),
        ),
	        tool(
	            "ios.action.back",
	            "Navigate back (best-effort on native apps).",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" }
	                },
	                "additionalProperties": false
	            }),
	        ),
	        tool(
	            "ios.action.scroll_until",
	            "Scroll until a target element exists (composite: find -> scroll -> retry).",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" },
	                    "target": {
	                        "type": "object",
	                        "properties": {
	                            "encodedId": { "type": "string" },
	                            "snapshotId": { "type": "string" },
	                            "using": { "type": "string" },
	                            "value": { "type": "string" },
	                            "index": { "type": "integer", "minimum": 0, "default": 0 },
	                            "requireUnique": { "type": "boolean", "default": false }
	                        },
	                        "additionalProperties": false
	                    },
	                    "direction": { "type": "string", "enum": ["up", "down", "left", "right"], "default": "down" },
	                    "distance": { "type": "number", "default": 0.6 },
	                    "maxScrolls": { "type": "integer", "default": 12 },
	                    "timeoutMs": { "type": "integer", "default": 60000 },
	                    "settleMs": { "type": "integer", "default": 350 }
	                },
	                "required": ["target"],
	                "additionalProperties": false
	            }),
	        ),
	        tool(
	            "ios.element.text",
	            "Read element text by locator or encoded id (read-only).",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" },
	                    "target": {
	                        "type": "object",
	                        "properties": {
	                            "encodedId": { "type": "string" },
	                            "snapshotId": { "type": "string" },
	                            "using": { "type": "string" },
	                            "value": { "type": "string" },
	                            "index": { "type": "integer", "minimum": 0, "default": 0 },
	                            "requireUnique": { "type": "boolean", "default": false }
	                        },
	                        "additionalProperties": false
	                    }
	                },
	                "required": ["target"],
	                "additionalProperties": false
	            }),
	        ),
	        tool(
	            "ios.element.attribute",
	            "Read an element attribute by locator or encoded id (read-only).",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" },
	                    "target": {
	                        "type": "object",
	                        "properties": {
	                            "encodedId": { "type": "string" },
	                            "snapshotId": { "type": "string" },
	                            "using": { "type": "string" },
	                            "value": { "type": "string" },
	                            "index": { "type": "integer", "minimum": 0, "default": 0 },
	                            "requireUnique": { "type": "boolean", "default": false }
	                        },
	                        "additionalProperties": false
	                    },
	                    "name": { "type": "string" }
	                },
	                "required": ["target", "name"],
	                "additionalProperties": false
	            }),
	        ),
	        tool(
	            "ios.element.rect",
	            "Read an element rect (x/y/width/height) by locator or encoded id (read-only).",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" },
	                    "target": {
	                        "type": "object",
	                        "properties": {
	                            "encodedId": { "type": "string" },
	                            "snapshotId": { "type": "string" },
	                            "using": { "type": "string" },
	                            "value": { "type": "string" },
	                            "index": { "type": "integer", "minimum": 0, "default": 0 },
	                            "requireUnique": { "type": "boolean", "default": false }
	                        },
	                        "additionalProperties": false
	                    }
	                },
	                "required": ["target"],
	                "additionalProperties": false
	            }),
	        ),
	        tool(
	            "ios.alert.text",
	            "Read the currently displayed system alert text, if any (read-only).",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" }
	                },
	                "additionalProperties": false
	            }),
	        ),
	        tool(
	            "ios.alert.accept",
	            "Accept the currently displayed system alert, if any.",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" }
	                },
	                "additionalProperties": false
	            }),
	        ),
	        tool(
	            "ios.alert.dismiss",
	            "Dismiss the currently displayed system alert, if any.",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" }
	                },
	                "additionalProperties": false
	            }),
	        ),
	        tool(
	            "ios.alert.wait",
	            "Wait until a system alert is present and return its text (read-only).",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" },
	                    "timeoutMs": { "type": "integer", "default": 10000 }
	                },
	                "additionalProperties": false
	            }),
	        ),
	        tool(
	            "ios.reddit.open_first_post",
	            "Open the first non-promoted Reddit feed post (best-effort).",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" },
	                    "maxCandidates": { "type": "integer", "default": 8 },
	                    "skipPromoted": { "type": "boolean", "default": true }
	                },
	                "additionalProperties": false
	            }),
	        ),
	        tool(
	            "ios.reddit.extract_post",
	            "Extract best-effort post details from the current Reddit post screen (read-only).",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" },
	                    "maxComments": { "type": "integer", "default": 3 },
	                    "maxRawLines": { "type": "integer", "default": 80 }
	                },
	                "additionalProperties": false
	            }),
	        ),
	        tool(
	            "ios.web.goto",
	            "Navigate Safari session to a URL.",
	            json!({
	                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "url": { "type": "string" }
                },
                "required": ["url"],
                "additionalProperties": false
            }),
        ),
	        tool(
	            "ios.web.wait_css",
	            "Wait for a CSS selector and return a matching element id.",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" },
	                    "selector": { "type": "string" },
	                    "index": { "type": "integer", "minimum": 0, "default": 0 },
	                    "requireUnique": { "type": "boolean", "default": false },
	                    "timeoutMs": { "type": "integer", "default": 10000 }
	                },
	                "required": ["selector"],
	                "additionalProperties": false
            }),
        ),
	        tool(
	            "ios.web.click_css",
	            "Click an element matching a CSS selector.",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" },
	                    "selector": { "type": "string" },
	                    "index": { "type": "integer", "minimum": 0, "default": 0 },
	                    "requireUnique": { "type": "boolean", "default": false }
	                },
	                "required": ["selector"],
	                "additionalProperties": false
            }),
        ),
	        tool(
	            "ios.web.type_css",
	            "Type text into an element matching a CSS selector.",
	            json!({
	                "type": "object",
	                "properties": {
	                    "sessionId": { "type": "string" },
	                    "selector": { "type": "string" },
	                    "index": { "type": "integer", "minimum": 0, "default": 0 },
	                    "requireUnique": { "type": "boolean", "default": false },
	                    "text": { "type": "string" },
	                    "clearFirst": { "type": "boolean", "default": true }
	                },
                "required": ["selector", "text"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.web.press_key",
            "Send a keyboard key to the active element (supports Enter for MVP).",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "key": { "type": "string", "default": "Enter" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.web.page_source",
            "Get current page source.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.web.screenshot",
            "Capture a screenshot from the active session.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.web.eval_js",
            "UNSAFE (high-risk): execute raw JavaScript in the current page context.",
            json!({
                "type": "object",
                "properties": {
                    "sessionId": { "type": "string" },
                    "script": { "type": "string" },
                    "args": { "type": "array", "items": {} }
                },
                "required": ["script"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.workflow.list",
            "List prebuilt iOS workflows.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
        tool(
            "ios.workflow.run",
            "Run a named workflow.",
            json!({
	                "type": "object",
	                "properties": {
	                    "name": { "type": "string" },
	                    "session": { "type": "object" },
	                    "args": { "type": "object" },
	                    "commit": { "type": "boolean", "default": false },
	                    "closeOnFinish": { "type": "boolean", "default": true },
	                    "stopAppiumOnFinish": { "type": "boolean", "default": false }
	                },
	                "required": ["name"],
	                "additionalProperties": false
            }),
        ),
        tool(
            "util.rank_by_name",
            "Compute a 1-based rank for a target string in a list of items (generic helper).",
            json!({
                "type": "object",
                "properties": {
                    "items": { "type": "array" },
                    "field": { "type": "string", "default": "name" },
                    "target": { "type": "string" }
                },
                "required": ["items", "target"],
                "additionalProperties": false
            }),
        ),
        tool(
            "util.list.length",
            "Return the length of an array.",
            json!({
                "type": "object",
                "properties": {
                    "list": { "type": "array" }
                },
                "required": ["list"],
                "additionalProperties": false
            }),
        ),
        tool(
            "ios.script.run",
            "Execute a deterministic step list (each step calls an existing tool).",
            json!({
	                "type": "object",
	                "properties": {
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "tool": { "type": "string" },
                                "arguments": { "type": "object" },
                                "timeoutMs": { "type": "integer" },
                                "retries": { "type": "integer", "default": 0 },
                                "requiresCommit": { "type": "boolean", "default": false },
                                "saveAs": { "type": "string" },
                                "save_as": { "type": "string" }
                            },
                            "required": ["tool"],
                            "additionalProperties": false
                        }
                    },
	                    "vars": { "type": "object", "default": {} },
	                    "commit": { "type": "boolean", "default": false },
	                    "closeOnFinish": { "type": "boolean", "default": true },
	                    "stopAppiumOnFinish": { "type": "boolean", "default": false }
	                },
	                "required": ["steps"],
	                "additionalProperties": false
	            }),
	        ),
    ]
}

pub async fn handle_tool_call(
    state: &AppState,
    tool_name: &str,
    arguments: Value,
) -> Result<Value> {
    match tool_name {
        "rzn.worker.health" => worker_health(state).await,
        "rzn.worker.shutdown" => worker_shutdown(state, &arguments).await,
        "ios.env.doctor" => env_doctor().await,
        "ios.device.list" => device_list(&arguments).await,
        "ios.appium.ensure" => appium_ensure(state, &arguments).await,
        "ios.session.create" => session_create(state, &arguments).await,
        "ios.session.delete" => session_delete(state, &arguments).await,
        "ios.session.info" => session_info(state).await,
        "ios.wda.shutdown" => wda_shutdown(state, &arguments).await,
        "ios.ui.source" => ui_source(state, &arguments).await,
        "ios.ui.screenshot" => ui_screenshot(state, &arguments).await,
        "ios.ui.observe_compact" => ui_observe_compact(state, &arguments).await,
        "ios.ui.extract_rows" => ui_extract_rows(state, &arguments).await,
        "ios.target.resolve" => target_resolve(state, &arguments).await,
        "ios.action.tap" => action_tap(state, &arguments).await,
        "ios.action.type" => action_type(state, &arguments).await,
        "ios.action.typeahead" => action_typeahead(state, &arguments).await,
        "ios.action.wait" => action_wait(state, &arguments).await,
        "ios.action.scroll" => action_scroll(state, &arguments).await,
        "ios.action.swipe" => action_swipe(state, &arguments).await,
        "ios.action.back" => action_back(state, &arguments).await,
        "ios.action.scroll_until" => action_scroll_until(state, &arguments).await,
        "ios.element.text" => element_text(state, &arguments).await,
        "ios.element.attribute" => element_attribute(state, &arguments).await,
        "ios.element.rect" => element_rect(state, &arguments).await,
        "ios.alert.text" => alert_text(state, &arguments).await,
        "ios.alert.accept" => alert_accept(state, &arguments).await,
        "ios.alert.dismiss" => alert_dismiss(state, &arguments).await,
        "ios.alert.wait" => alert_wait(state, &arguments).await,
        "ios.reddit.open_first_post" => reddit_open_first_post(state, &arguments).await,
        "ios.reddit.extract_post" => reddit_extract_post(state, &arguments).await,
        "ios.web.goto" => web_goto(state, &arguments).await,
        "ios.web.wait_css" => web_wait_css(state, &arguments).await,
        "ios.web.click_css" => web_click_css(state, &arguments).await,
        "ios.web.type_css" => web_type_css(state, &arguments).await,
        "ios.web.press_key" => web_press_key(state, &arguments).await,
        "ios.web.page_source" => web_page_source(state, &arguments).await,
        "ios.web.screenshot" => web_screenshot(state, &arguments).await,
        "ios.web.eval_js" => web_eval_js(state, &arguments).await,
        "ios.workflow.list" => workflow_list().await,
        "ios.workflow.run" => workflow_run(state, &arguments).await,
        "ios.script.run" => script_run(state, &arguments).await,
        "util.rank_by_name" => util_rank_by_name(&arguments).await,
        "util.list.length" => util_list_length(&arguments).await,
        _ => bail!("unknown tool '{tool_name}'"),
    }
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn tool_success(structured: Value, message: &str) -> Value {
    json!({
        "content": [
            { "type": "text", "text": message }
        ],
        "structuredContent": structured
    })
}

fn tool_success_with_content(structured: Value, mut content: Vec<Value>) -> Value {
    if content.is_empty() {
        content.push(json!({"type": "text", "text": "ok"}));
    }
    json!({
        "content": content,
        "structuredContent": structured
    })
}

pub fn tool_error_result(message: &str, details: Value) -> Value {
    tool_error_result_with_code(message, None, details)
}

pub fn tool_error_result_with_code(message: &str, error_code: Option<&str>, details: Value) -> Value {
    json!({
        "isError": true,
        "content": [
            { "type": "text", "text": message }
        ],
        "structuredContent": {
            "ok": false,
            "error": message,
            "errorCode": error_code,
            "details": details
        }
    })
}

pub fn tool_error_from_anyhow(err: &anyhow::Error, tool: &str) -> Value {
    if let Some(typed) = err.downcast_ref::<ToolCallError>() {
        return tool_error_result_with_code(
            &typed.message,
            Some(typed.code.as_str()),
            merge_error_details(tool, &typed.details),
        );
    }

    let message = format!("{err:#}");
    let lowered = message.to_lowercase();
    let code = if lowered.contains("timeout") {
        ToolErrorCode::Timeout
    } else if lowered.contains("device was not, or could not be, unlocked")
        || lowered.contains("could not be unlocked")
        || lowered.contains("bserrorcodedescription=locked")
        || lowered.contains(" for reason: locked")
    {
        ToolErrorCode::DeviceLocked
    } else if lowered.contains("no active session")
        || lowered.contains("sessionid is required")
        || lowered.contains("appium is not initialized")
    {
        ToolErrorCode::NoSession
    } else if lowered.contains("requires commit") {
        ToolErrorCode::CommitRequired
    } else if lowered.contains("no elements found") || lowered.contains("no matching elements") {
        ToolErrorCode::ElementNotFound
    } else if lowered.contains("expected exactly one match")
        || lowered.contains("multiple matching elements")
        || lowered.contains("ambiguous")
    {
        ToolErrorCode::AmbiguousMatch
    } else if lowered.contains("required") || lowered.contains("invalid params") {
        ToolErrorCode::InvalidParams
    } else {
        ToolErrorCode::Internal
    };

    tool_error_result_with_code(&message, Some(code.as_str()), json!({ "tool": tool }))
}

fn merge_error_details(tool: &str, details: &Value) -> Value {
    let mut merged = serde_json::Map::new();
    merged.insert("tool".to_string(), json!(tool));

    if let Some(obj) = details.as_object() {
        for (k, v) in obj {
            merged.insert(k.clone(), v.clone());
        }
    } else if !details.is_null() {
        merged.insert("details".to_string(), details.clone());
    }

    Value::Object(merged)
}

async fn worker_health(state: &AppState) -> Result<Value> {
    let snapshot = state.snapshot().await;
    let appium_health = if let Some(base_url) = snapshot.appium_base_url.clone() {
        probe_webdriver_base(&base_url).await.is_ok()
    } else {
        false
    };

    let source = snapshot
        .appium_source
        .map(|src| match src {
            crate::state::AppiumSource::Env => "env",
            crate::state::AppiumSource::Spawned => "spawned",
        })
        .unwrap_or("none");

    Ok(tool_success(
        json!({
            "ok": true,
            "id": "ios-tools/ios",
            "plugin_version": std::env::var("RZN_PLUGIN_VERSION").unwrap_or_else(|_| "dev".to_string()),
            "mcp_protocol_version": "2025-06-18",
            "ready": true,
            "appium": {
                "running": appium_health,
                "baseUrl": snapshot.appium_base_url,
                "pid": snapshot.appium_pid,
                "source": source
            },
            "active_session": snapshot.session
        }),
        "worker healthy",
    ))
}

async fn worker_shutdown(state: &AppState, arguments: &Value) -> Result<Value> {
    let stop_appium = arguments
        .get("stopAppium")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let shutdown_wda = arguments
        .get("shutdownWDA")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let snapshot = state.snapshot().await;
    let wda_port = state
        .last_wda_local_port()
        .await
        .unwrap_or(DEFAULT_WDA_LOCAL_PORT);

    let mut closed_session = false;
    let mut stopped_env_appium = false;
    let mut wda_shutdown_ok = false;

    if let Some(active) = state.active_session().await {
        if let Some(base_url) = snapshot.appium_base_url.clone() {
            let driver = WebDriverClient::new(&base_url)?;
            let _ = driver.delete_session(&active.session_id).await;
        }
        state.clear_session().await;
        closed_session = true;
    }

    if shutdown_wda {
        wda_shutdown_ok = shutdown_wda_on_port(wda_port).await.unwrap_or(false);
    }

    if stop_appium {
        match snapshot.appium_source {
            Some(AppiumSource::Spawned) => {
                state.shutdown_spawned_appium().await;
            }
            Some(AppiumSource::Env) => {
                if let Some(base_url) = snapshot.appium_base_url.as_deref() {
                    stopped_env_appium = stop_local_env_appium(base_url).await.unwrap_or(false);
                }
                state.clear_appium_metadata().await;
            }
            None => {}
        }
    } else {
        state.clear_session().await;
    }

    Ok(tool_success(
        json!({
            "ok": true,
            "stopAppium": stop_appium,
            "shutdownWDA": shutdown_wda,
            "wdaLocalPort": wda_port,
            "wdaShutdownOk": wda_shutdown_ok,
            "closedSession": closed_session,
            "stoppedEnvAppium": stopped_env_appium
        }),
        "shutdown complete",
    ))
}

fn parse_localhost_port(base_url: &str) -> Option<u16> {
    let remainder = base_url.split("://").nth(1).unwrap_or(base_url);
    let authority = remainder.split('/').next()?;

    if authority.starts_with('[') {
        let host_end = authority.find(']')?;
        let host = &authority[1..host_end];
        let port = authority[host_end + 1..].strip_prefix(':')?;
        if host == "::1" {
            return port.parse::<u16>().ok();
        }
        return None;
    }

    let mut parts = authority.split(':');
    let host = parts.next()?;
    let port = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    match host {
        "localhost" | "127.0.0.1" => port.parse::<u16>().ok(),
        _ => None,
    }
}

async fn stop_local_env_appium(base_url: &str) -> Result<bool> {
    let Some(port) = parse_localhost_port(base_url) else {
        return Ok(false);
    };

    let port_flag = format!("-iTCP:{port}");
    let output = Command::new("lsof")
        .args(["-nP", &port_flag, "-sTCP:LISTEN", "-t"])
        .output()
        .await
        .context("run lsof for Appium port")?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let pids: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect();

    if pids.is_empty() {
        return Ok(false);
    }

    for pid in &pids {
        let _ = Command::new("kill").args(["-TERM", pid]).status().await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;

    for pid in &pids {
        if Command::new("kill")
            .args(["-0", pid])
            .status()
            .await
            .map(|status| status.success())
            .unwrap_or(false)
        {
            let _ = Command::new("kill").args(["-KILL", pid]).status().await;
        }
    }

    Ok(true)
}

async fn shutdown_wda_on_port(port: u16) -> Result<bool> {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .context("build HTTP client")?;

    let urls = [
        format!("http://127.0.0.1:{port}/wda/shutdown"),
        format!("http://localhost:{port}/wda/shutdown"),
        format!("http://[::1]:{port}/wda/shutdown"),
    ];

    for url in urls {
        let response = match client.get(&url).send().await {
            Ok(response) => response,
            Err(_) => continue,
        };

        if response.status().is_success() {
            return Ok(true);
        }
    }

    Ok(false)
}

async fn env_doctor() -> Result<Value> {
    let mut checks = Vec::new();

    checks.push(run_check("xcodebuild", "xcodebuild", &["-version"], None).await);
    checks.push(run_check("xctrace", "xcrun", &["xctrace", "list", "devices"], None).await);
    checks.push(run_check("node", "node", &["--version"], None).await);
    checks.push(run_check("appium", "appium", &["--version"], None).await);
    checks.push(
        run_check(
            "appium_xcuitest_driver",
            "appium",
            &["driver", "list", "--installed"],
            Some("xcuitest"),
        )
        .await,
    );

    let ok = checks
        .iter()
        .all(|entry| entry.get("ok") == Some(&Value::Bool(true)));

    Ok(tool_success(
        json!({
            "ok": ok,
            "checks": checks,
            "remediation": [
                "Install Node.js LTS and ensure it is available to GUI-launched apps.",
                "Install Appium: npm i -g appium",
                "Install XCUITest driver: appium driver install xcuitest",
                "Prefer setting RZN_IOS_APPIUM_URL for desktop runtime stability."
            ]
        }),
        if ok {
            "environment looks good"
        } else {
            "environment has missing prerequisites"
        },
    ))
}

async fn run_check(
    name: &str,
    command: &str,
    args: &[&str],
    output_must_contain: Option<&str>,
) -> Value {
    let output = Command::new(command).args(args).output().await;
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let mut ok = output.status.success();
            if let Some(fragment) = output_must_contain {
                let haystack = format!("{stdout}\n{stderr}").to_lowercase();
                ok = ok && haystack.contains(&fragment.to_lowercase());
            }
            json!({
                "name": name,
                "ok": ok,
                "exitCode": output.status.code(),
                "stdout": stdout,
                "stderr": stderr
            })
        }
        Err(err) => json!({
            "name": name,
            "ok": false,
            "error": err.to_string()
        }),
    }
}

async fn device_list(arguments: &Value) -> Result<Value> {
    let include_simulators = arguments
        .get("includeSimulators")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let devices = xctrace::list_devices(include_simulators).await?;

    Ok(tool_success(
        json!({ "devices": devices }),
        "device list complete",
    ))
}

async fn appium_ensure(state: &AppState, arguments: &Value) -> Result<Value> {
    let port = arguments
        .get("port")
        .and_then(Value::as_u64)
        .map(|value| value as u16);
    let log_level = arguments
        .get("logLevel")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    let result = ensure_appium(state, EnsureOptions { port, log_level }).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "baseUrl": result.base_url,
            "source": result.source,
            "pid": result.pid
        }),
        "appium ready",
    ))
}

async fn session_create(state: &AppState, arguments: &Value) -> Result<Value> {
    let udid = required_str(arguments, "udid")?.to_string();
    let kind = arguments
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("safari_web");
    if kind != "safari_web" && kind != "native_app" {
        bail!("unsupported session kind '{kind}'");
    }

    let replace_existing = arguments
        .get("replaceExisting")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let ensure_result = ensure_appium(
        state,
        EnsureOptions {
            port: None,
            log_level: None,
        },
    )
    .await?;

    let driver = WebDriverClient::new(&ensure_result.base_url)?;

    if replace_existing {
        if let Some(existing) = state.active_session().await {
            let _ = driver.delete_session(&existing.session_id).await;
            state.clear_session().await;
        }
    }

    let signing = arguments
        .get("signing")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let wda_local_port = arguments
        .get("wdaLocalPort")
        .and_then(Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .filter(|v| *v > 0);

    let request = SessionCreateRequest {
        udid: udid.clone(),
        no_reset: arguments.get("noReset").and_then(Value::as_bool).unwrap_or(true),
        new_command_timeout_sec: arguments
            .get("newCommandTimeoutSec")
            .and_then(Value::as_u64)
            .unwrap_or(60),
        session_create_timeout_ms: Some(
            arguments
                .get("sessionCreateTimeoutMs")
                .and_then(Value::as_u64)
                .unwrap_or(600_000),
        ),
        wda_local_port,
        wda_launch_timeout_ms: Some(
            arguments
                .get("wdaLaunchTimeoutMs")
                .and_then(Value::as_u64)
                .unwrap_or(240_000),
        ),
        wda_connection_timeout_ms: Some(
            arguments
                .get("wdaConnectionTimeoutMs")
                .and_then(Value::as_u64)
                .unwrap_or(120_000),
        ),
        show_xcode_log: arguments.get("showXcodeLog").and_then(Value::as_bool),
        allow_provisioning_updates: arguments
            .get("allowProvisioningUpdates")
            .and_then(Value::as_bool),
        allow_provisioning_device_registration: arguments
            .get("allowProvisioningDeviceRegistration")
            .and_then(Value::as_bool),
        language: arguments
            .get("language")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        locale: arguments
            .get("locale")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        xcode_org_id: signing
            .get("xcodeOrgId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        xcode_signing_id: signing
            .get("xcodeSigningId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        updated_wda_bundle_id: signing
            .get("updatedWDABundleId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
    };

    let created = match kind {
        "safari_web" => driver
            .create_session_safari(request)
            .await
            .context("failed to create Safari session")?,
        "native_app" => {
            let bundle_id = required_str(arguments, "bundleId")?.to_string();
            driver
                .create_session_native_app(request, bundle_id)
                .await
                .context("failed to create native app session")?
        }
        _ => unreachable!(),
    };

    state
        .set_session(
            created.session_id.clone(),
            kind.to_string(),
            udid,
            wda_local_port,
        )
        .await;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": created.session_id,
            "kind": kind,
            "appiumBaseUrl": ensure_result.base_url,
            "capabilities": created.capabilities
        }),
        "session created",
    ))
}

async fn ui_source(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    let source = driver.page_source(&session_id).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "length": source.len(),
            "source": source
        }),
        "ui source captured",
    ))
}

async fn ui_screenshot(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    let data = driver.screenshot(&session_id).await?;

    Ok(tool_success_with_content(
        json!({
            "ok": true,
            "sessionId": session_id,
            "mimeType": "image/png",
            "bytesBase64": data.len(),
            "data": data
        }),
        vec![
            json!({"type": "text", "text": "screenshot captured"}),
            json!({"type": "image", "mimeType": "image/png", "data": data}),
        ],
    ))
}

async fn ui_observe_compact(state: &AppState, arguments: &Value) -> Result<Value> {
    let session = state
        .active_session()
        .await
        .ok_or_else(|| anyhow!("no active session; call ios.session.create first"))?;
    if session.kind != "native_app" {
        bail!(
            "ios.ui.observe_compact requires a native_app session (current kind={})",
            session.kind
        );
    }

    let session_id = resolve_session_id(state, arguments).await?;
    if session_id != session.session_id {
        bail!("unknown sessionId (this worker supports a single active session)");
    }

    let filter = arguments
        .get("nodeFilter")
        .and_then(Value::as_str)
        .map(NodeFilter::from_str)
        .unwrap_or(NodeFilter::Interactive);
    let max_nodes = arguments
        .get("maxNodes")
        .and_then(Value::as_u64)
        .unwrap_or(140)
        .clamp(10, 500) as usize;

    let driver = driver_from_state(state).await?;
    let source = driver.page_source(&session_id).await?;
    let snapshot = build_compact_snapshot(&source, filter, max_nodes)
        .context("failed to build compact snapshot (is this native XML source?)")?;

    let snapshot_id = snapshot.snapshot_id.clone();
    state
        .set_compact_observation(snapshot_id.clone(), session_id.clone(), snapshot.targets)
        .await;

    let nodes_json = serde_json::to_value(&snapshot.nodes).unwrap_or_else(|_| json!([]));

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "snapshotId": snapshot_id,
            "nodes": nodes_json,
            "stats": snapshot.stats
        }),
        "compact snapshot captured",
    ))
}

async fn ui_extract_rows(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;

    let source_override = arguments.get("source").and_then(Value::as_str).map(|raw| raw.to_string());

    let row_query = parse_row_query(arguments.get("row"))?;
    let primary_query = parse_primary_query(arguments.get("primary"))?;
    let tag_query = parse_tag_query(arguments.get("tag"));
    let split_cfg = parse_split_config(arguments.get("split"));
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .filter(|value| *value > 0);
    let order = arguments
        .get("order")
        .and_then(Value::as_str)
        .unwrap_or("y")
        .to_lowercase();
    let max_scrolls = arguments
        .get("maxScrolls")
        .and_then(Value::as_u64)
        .or_else(|| arguments.get("max_scrolls").and_then(Value::as_u64))
        .unwrap_or(0)
        .clamp(0, 50) as u32;
    if source_override.is_some() && max_scrolls > 0 {
        bail!("source cannot be combined with maxScrolls");
    }

    let (scroll_direction, scroll_distance, settle_ms) = if let Some(scroll) =
        arguments.get("scroll").and_then(Value::as_object)
    {
        let direction = scroll
            .get("direction")
            .and_then(Value::as_str)
            .unwrap_or("down")
            .to_lowercase();
        let distance = scroll
            .get("distance")
            .and_then(Value::as_f64)
            .unwrap_or(0.6)
            .clamp(0.1, 0.95);
        let settle_ms = scroll
            .get("settleMs")
            .and_then(Value::as_u64)
            .unwrap_or(350)
            .clamp(0, 10_000);
        (direction, distance, settle_ms)
    } else {
        ("down".to_string(), 0.6, 350)
    };

    let mut rows_out: Vec<RowMatch> = Vec::new();
    let mut seen = HashSet::<String>::new();
    let mut scrolls_done: u32 = 0;

    for pass in 0..=max_scrolls {
        let source = if let Some(raw) = source_override.as_ref() {
            raw.clone()
        } else {
            driver.page_source(&session_id).await?
        };
        let mut rows = extract_rows_from_source(
            &source,
            &row_query,
            &primary_query,
            tag_query.as_ref(),
            &split_cfg,
        );

        if order == "x" {
            rows.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        } else {
            rows.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
        }

        for row in rows {
            let key = normalize_match_key(&row.raw_label);
            if key.is_empty() || !seen.insert(key) {
                continue;
            }
            rows_out.push(row);
            if let Some(max) = limit {
                if rows_out.len() >= max {
                    break;
                }
            }
        }

        if let Some(max) = limit {
            if rows_out.len() >= max {
                break;
            }
        }
        if source_override.is_some() {
            break;
        }
        if pass < max_scrolls {
            perform_scroll_gesture(&driver, &session_id, &scroll_direction, scroll_distance).await?;
            scrolls_done += 1;
            if settle_ms > 0 {
                tokio::time::sleep(Duration::from_millis(settle_ms)).await;
            }
        }
    }

    let output_rows: Vec<Value> = rows_out
        .into_iter()
        .enumerate()
        .map(|(idx, row)| {
            let mut obj = serde_json::Map::new();
            obj.insert("position".to_string(), json!(idx + 1));
            for (k, v) in row.fields {
                obj.insert(k, json!(v));
            }
            if let Some(tag_field) = row.tag_field {
                obj.insert(tag_field, json!(row.tag_value.unwrap_or_default()));
            }
            obj.insert("rawLabel".to_string(), json!(row.raw_label));
            Value::Object(obj)
        })
        .collect();

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "rowCount": output_rows.len(),
            "rows": output_rows,
            "scrolls": scrolls_done
        }),
        "rows extracted",
    ))
}

async fn target_resolve(state: &AppState, arguments: &Value) -> Result<Value> {
    let encoded_id = required_str(arguments, "encodedId")?;
    let snapshot_id = arguments.get("snapshotId").and_then(Value::as_str);

    let current_snapshot = state
        .compact_snapshot_id()
        .await
        .unwrap_or_else(|| "<none>".to_string());

    let locator = state
        .resolve_compact_target(snapshot_id, encoded_id)
        .await
        .ok_or_else(|| {
            anyhow!(
                "unable to resolve encodedId '{encoded_id}'. Re-run ios.ui.observe_compact (current snapshotId={current_snapshot})."
            )
        })?;

    Ok(tool_success(
        json!({
            "ok": true,
            "encodedId": encoded_id,
            "targetSpec": locator_to_json(&locator)
        }),
        "target resolved",
    ))
}

#[derive(Debug, Clone)]
struct ResolvedTarget {
    using: String,
    value: String,
    index: usize,
    require_unique: bool,
}

async fn resolve_target(state: &AppState, arguments: &Value) -> Result<Option<ResolvedTarget>> {
    if let Some(point) = arguments.get("point") {
        if point.get("x").is_some() || point.get("y").is_some() {
            return Ok(None);
        }
    }

    let Some(target) = arguments.get("target").and_then(Value::as_object) else {
        return Err(ToolCallError::new(
            ToolErrorCode::InvalidParams,
            "target is required (or provide point)",
            json!({}),
        )
        .into());
    };

    let index = match target.get("index") {
        Some(value) => {
            if let Some(u) = value.as_u64() {
                u as usize
            } else if let Some(i) = value.as_i64() {
                if i < 0 {
                    return Err(ToolCallError::new(
                        ToolErrorCode::InvalidParams,
                        "target.index must be >= 0",
                        json!({"index": i}),
                    )
                    .into());
                }
                i as usize
            } else {
                return Err(ToolCallError::new(
                    ToolErrorCode::InvalidParams,
                    "target.index must be an integer",
                    json!({"index": value}),
                )
                .into());
            }
        }
        None => 0,
    };

    let require_unique = target
        .get("requireUnique")
        .and_then(Value::as_bool)
        .or_else(|| target.get("require_unique").and_then(Value::as_bool))
        .unwrap_or(false);

    if let Some(encoded) = target.get("encodedId").and_then(Value::as_str).map(str::trim) {
        if !encoded.is_empty() {
            let snapshot_id = target.get("snapshotId").and_then(Value::as_str);
            let locator = state
                .resolve_compact_target(snapshot_id, encoded)
                .await
                .ok_or_else(|| {
                    anyhow!("encodedId '{encoded}' not found. Call ios.ui.observe_compact first.")
                })?;
            return Ok(Some(ResolvedTarget {
                using: locator.using,
                value: locator.value,
                index,
                require_unique,
            }));
        }
    }

    if let (Some(using), Some(value)) = (
        target.get("using").and_then(Value::as_str).map(str::trim),
        target.get("value").and_then(Value::as_str).map(str::trim),
    ) {
        if using.is_empty() || value.is_empty() {
            return Err(ToolCallError::new(
                ToolErrorCode::InvalidParams,
                "target.using and target.value must be non-empty",
                json!({ "using": using, "value": value }),
            )
            .into());
        }
        return Ok(Some(ResolvedTarget {
            using: using.to_string(),
            value: value.to_string(),
            index,
            require_unique,
        }));
    }

    Err(ToolCallError::new(
        ToolErrorCode::InvalidParams,
        "target must include either encodedId or using/value (or provide point)",
        json!({}),
    )
    .into())
}

async fn action_tap(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;

    if let Some(point) = arguments.get("point") {
        let x = point.get("x").and_then(Value::as_f64).ok_or_else(|| anyhow!("point.x must be a number"))?;
        let y = point.get("y").and_then(Value::as_f64).ok_or_else(|| anyhow!("point.y must be a number"))?;
        driver.tap_point(&session_id, x, y).await?;
        return Ok(tool_success(
            json!({"ok": true, "sessionId": session_id, "point": {"x": x, "y": y}}),
            "tap complete",
        ));
    }

    let resolved = resolve_target(state, arguments)
        .await?
        .ok_or_else(|| ToolCallError::new(ToolErrorCode::InvalidParams, "target is required", json!({})))?;
    let ids = driver
        .find_elements(&session_id, &resolved.using, &resolved.value)
        .await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!(
                "no elements found for locator using='{}' value='{}'",
                &resolved.using, &resolved.value
            ),
            json!({"using": &resolved.using, "value": &resolved.value}),
        )
        .into());
    }
    if resolved.require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            format!(
                "expected exactly one match for using='{}' value='{}', got {}",
                &resolved.using,
                &resolved.value,
                ids.len()
            ),
            json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
        )
        .into());
    }

    let element_id = ids.get(resolved.index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!(
                "no element at index {} for locator using='{}' value='{}' (found {})",
                resolved.index,
                &resolved.using,
                &resolved.value,
                ids.len()
            ),
            json!({"using": &resolved.using, "value": &resolved.value, "index": resolved.index, "matchCount": ids.len()}),
        )
    })?;
    driver.click_element(&session_id, element_id).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "elementId": element_id,
            "targetSpec": {"using": &resolved.using, "value": &resolved.value, "index": resolved.index}
        }),
        "tap complete",
    ))
}

async fn action_type(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let text = required_str(arguments, "text")?;
    let clear_first = arguments.get("clearFirst").and_then(Value::as_bool).unwrap_or(true);

    let resolved = resolve_target(state, arguments)
        .await?
        .ok_or_else(|| ToolCallError::new(ToolErrorCode::InvalidParams, "target is required", json!({})))?;
    let driver = driver_from_state(state).await?;
    let ids = driver
        .find_elements(&session_id, &resolved.using, &resolved.value)
        .await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!(
                "no elements found for locator using='{}' value='{}'",
                &resolved.using, &resolved.value
            ),
            json!({"using": &resolved.using, "value": &resolved.value}),
        )
        .into());
    }
    if resolved.require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            format!(
                "expected exactly one match for using='{}' value='{}', got {}",
                &resolved.using,
                &resolved.value,
                ids.len()
            ),
            json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
        )
        .into());
    }

    let element_id = ids.get(resolved.index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!(
                "no element at index {} for locator using='{}' value='{}' (found {})",
                resolved.index,
                &resolved.using,
                &resolved.value,
                ids.len()
            ),
            json!({"using": &resolved.using, "value": &resolved.value, "index": resolved.index, "matchCount": ids.len()}),
        )
    })?;

    driver.click_element(&session_id, element_id).await?;
    if clear_first {
        let _ = driver.clear_element(&session_id, element_id).await;
    }
    driver.type_element(&session_id, element_id, text).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "elementId": element_id,
            "typedLength": text.chars().count(),
            "targetSpec": {"using": &resolved.using, "value": &resolved.value, "index": resolved.index}
        }),
        "type complete",
    ))
}

async fn action_typeahead(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let field = arguments
        .get("field")
        .cloned()
        .ok_or_else(|| ToolCallError::new(ToolErrorCode::InvalidParams, "field is required", json!({})))?;
    let typing_mode = arguments
        .get("typingMode")
        .and_then(Value::as_str)
        .unwrap_or("full")
        .to_lowercase();
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .clamp(1, 20) as usize;

    let prefixes = resolve_prefixes_for_typeahead(arguments)?;
    let suggestion_query = parse_node_query(arguments.get("suggestionQuery"));

    let driver = driver_from_state(state).await?;

    let mut prefixes_out = Vec::new();
    let mut final_suggestions = Vec::new();

    for prefix in &prefixes {
        type_into_field(state, &driver, &session_id, &field, prefix, &typing_mode).await?;
        tokio::time::sleep(Duration::from_millis(900)).await;

        let source = driver.page_source(&session_id).await?;
        let suggestions = extract_suggestion_texts(&source, &suggestion_query, limit);
        final_suggestions = suggestions.clone();
        prefixes_out.push(json!({
            "prefix": prefix,
            "suggestions": suggestions,
            "suggestionCount": suggestions.len()
        }));
    }

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "prefixes": prefixes_out,
            "prefixCount": prefixes.len(),
            "activePrefix": prefixes.last().cloned(),
            "suggestions": final_suggestions,
            "suggestionCount": final_suggestions.len(),
            "limit": limit
        }),
        "typeahead captured",
    ))
}

async fn action_wait(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let timeout_ms = arguments
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(10_000)
        .clamp(250, 180_000);

    let resolved = resolve_target(state, arguments)
        .await?
        .ok_or_else(|| ToolCallError::new(ToolErrorCode::InvalidParams, "target is required", json!({})))?;
    let driver = driver_from_state(state).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        let ids = driver
            .find_elements(&session_id, &resolved.using, &resolved.value)
            .await?;
        if ids.is_empty() {
            // keep waiting
        } else if resolved.require_unique && ids.len() != 1 {
            return Err(ToolCallError::new(
                ToolErrorCode::AmbiguousMatch,
                format!(
                    "expected exactly one match for using='{}' value='{}', got {}",
                    &resolved.using,
                    &resolved.value,
                    ids.len()
                ),
                json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
            )
            .into());
        } else if let Some(element_id) = ids.get(resolved.index) {
            return Ok(tool_success(
                json!({
                    "ok": true,
                    "sessionId": session_id,
                    "elementId": element_id,
                    "targetSpec": {"using": &resolved.using, "value": &resolved.value, "index": resolved.index}
                }),
                "element found",
            ));
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(ToolCallError::new(
                ToolErrorCode::Timeout,
                format!(
                    "timeout waiting for locator using='{}' value='{}'",
                    &resolved.using, &resolved.value
                ),
                json!({"using": &resolved.using, "value": &resolved.value, "timeoutMs": timeout_ms}),
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn action_scroll(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let direction = required_str(arguments, "direction")?.to_lowercase();
    let distance = arguments.get("distance").and_then(Value::as_f64).unwrap_or(0.6).clamp(0.1, 0.95);

    let driver = driver_from_state(state).await?;
    perform_scroll_gesture(&driver, &session_id, &direction, distance).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "direction": direction,
            "distance": distance
        }),
        "scroll complete",
    ))
}

async fn perform_scroll_gesture(
    driver: &WebDriverClient,
    session_id: &str,
    direction: &str,
    distance: f64,
) -> Result<()> {
    let distance = distance.clamp(0.1, 0.95);
    let (width, height) = driver.window_rect(session_id).await?;
    let (start_x, start_y, end_x, end_y) = match direction.trim().to_lowercase().as_str() {
        "down" => (
            width * 0.5,
            height * (0.5 + distance / 2.0),
            width * 0.5,
            height * (0.5 - distance / 2.0),
        ),
        "up" => (
            width * 0.5,
            height * (0.5 - distance / 2.0),
            width * 0.5,
            height * (0.5 + distance / 2.0),
        ),
        "left" => (
            width * (0.5 - distance / 2.0),
            height * 0.5,
            width * (0.5 + distance / 2.0),
            height * 0.5,
        ),
        "right" => (
            width * (0.5 + distance / 2.0),
            height * 0.5,
            width * (0.5 - distance / 2.0),
            height * 0.5,
        ),
        other => bail!("unsupported direction '{other}'"),
    };

    let payload = json!({
        "actions": [{
            "type": "pointer",
            "id": "finger1",
            "parameters": { "pointerType": "touch" },
            "actions": [
                {"type": "pointerMove", "duration": 0, "x": start_x, "y": start_y, "origin": "viewport"},
                {"type": "pointerDown", "button": 0},
                {"type": "pause", "duration": 100},
                {"type": "pointerMove", "duration": 400, "x": end_x, "y": end_y, "origin": "viewport"},
                {"type": "pointerUp", "button": 0}
            ]
        }]
    });
    driver.perform_actions(session_id, payload).await?;
    Ok(())
}

async fn action_swipe(state: &AppState, arguments: &Value) -> Result<Value> {
    action_scroll(state, arguments).await
}

async fn action_back(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    driver.back(&session_id).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id
        }),
        "back complete",
    ))
}

async fn action_scroll_until(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let direction = arguments
        .get("direction")
        .and_then(Value::as_str)
        .unwrap_or("down")
        .to_lowercase();
    let distance = arguments
        .get("distance")
        .and_then(Value::as_f64)
        .unwrap_or(0.6)
        .clamp(0.1, 0.95);
    let max_scrolls = arguments
        .get("maxScrolls")
        .and_then(Value::as_u64)
        .unwrap_or(12)
        .clamp(0, 200) as u32;
    let timeout_ms = arguments
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(60_000)
        .clamp(250, 600_000);
    let settle_ms = arguments
        .get("settleMs")
        .and_then(Value::as_u64)
        .unwrap_or(350)
        .clamp(0, 10_000);

    let resolved = resolve_target(state, arguments)
        .await?
        .ok_or_else(|| ToolCallError::new(ToolErrorCode::InvalidParams, "target is required", json!({})))?;
    let driver = driver_from_state(state).await?;

    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    let mut scrolls: u32 = 0;

    loop {
        let ids = driver
            .find_elements(&session_id, &resolved.using, &resolved.value)
            .await?;

        if !ids.is_empty() {
            if resolved.require_unique && ids.len() != 1 {
                return Err(ToolCallError::new(
                    ToolErrorCode::AmbiguousMatch,
                    format!(
                        "expected exactly one match for using='{}' value='{}', got {}",
                        &resolved.using,
                        &resolved.value,
                        ids.len()
                    ),
                    json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
                )
                .into());
            }

            if let Some(element_id) = ids.get(resolved.index) {
                return Ok(tool_success(
                    json!({
                        "ok": true,
                        "found": true,
                        "sessionId": session_id,
                        "elementId": element_id,
                        "scrolls": scrolls,
                        "targetSpec": {"using": &resolved.using, "value": &resolved.value, "index": resolved.index}
                    }),
                    "element found",
                ));
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(ToolCallError::new(
                ToolErrorCode::Timeout,
                "timeout scrolling to target",
                json!({"using": &resolved.using, "value": &resolved.value, "scrolls": scrolls, "timeoutMs": timeout_ms}),
            )
            .into());
        }
        if scrolls >= max_scrolls {
            return Err(ToolCallError::new(
                ToolErrorCode::ElementNotFound,
                "target not found within maxScrolls",
                json!({"using": &resolved.using, "value": &resolved.value, "scrolls": scrolls, "maxScrolls": max_scrolls}),
            )
            .into());
        }

        perform_scroll_gesture(&driver, &session_id, &direction, distance).await?;
        scrolls += 1;
        if settle_ms > 0 {
            tokio::time::sleep(Duration::from_millis(settle_ms)).await;
        }
    }
}

async fn element_text(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let resolved = resolve_target(state, arguments)
        .await?
        .ok_or_else(|| ToolCallError::new(ToolErrorCode::InvalidParams, "target is required", json!({})))?;
    let driver = driver_from_state(state).await?;

    let ids = driver
        .find_elements(&session_id, &resolved.using, &resolved.value)
        .await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "no matching elements",
            json!({"using": &resolved.using, "value": &resolved.value}),
        )
        .into());
    }
    if resolved.require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            "multiple matching elements",
            json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
        )
        .into());
    }
    let element_id = ids.get(resolved.index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "target index out of bounds",
            json!({"using": &resolved.using, "value": &resolved.value, "index": resolved.index, "matchCount": ids.len()}),
        )
    })?;

    let text = driver.element_text(&session_id, element_id).await?;
    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "elementId": element_id,
            "text": text
        }),
        "element text read",
    ))
}

async fn element_attribute(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let name = required_str(arguments, "name")?;
    let resolved = resolve_target(state, arguments)
        .await?
        .ok_or_else(|| ToolCallError::new(ToolErrorCode::InvalidParams, "target is required", json!({})))?;
    let driver = driver_from_state(state).await?;

    let ids = driver
        .find_elements(&session_id, &resolved.using, &resolved.value)
        .await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "no matching elements",
            json!({"using": &resolved.using, "value": &resolved.value}),
        )
        .into());
    }
    if resolved.require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            "multiple matching elements",
            json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
        )
        .into());
    }
    let element_id = ids.get(resolved.index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "target index out of bounds",
            json!({"using": &resolved.using, "value": &resolved.value, "index": resolved.index, "matchCount": ids.len()}),
        )
    })?;

    let value = driver.element_attribute(&session_id, element_id, name).await?;
    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "elementId": element_id,
            "name": name,
            "value": value
        }),
        "element attribute read",
    ))
}

async fn element_rect(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let resolved = resolve_target(state, arguments)
        .await?
        .ok_or_else(|| ToolCallError::new(ToolErrorCode::InvalidParams, "target is required", json!({})))?;
    let driver = driver_from_state(state).await?;

    let ids = driver
        .find_elements(&session_id, &resolved.using, &resolved.value)
        .await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "no matching elements",
            json!({"using": &resolved.using, "value": &resolved.value}),
        )
        .into());
    }
    if resolved.require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            "multiple matching elements",
            json!({"using": &resolved.using, "value": &resolved.value, "matchCount": ids.len()}),
        )
        .into());
    }
    let element_id = ids.get(resolved.index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "target index out of bounds",
            json!({"using": &resolved.using, "value": &resolved.value, "index": resolved.index, "matchCount": ids.len()}),
        )
    })?;

    let rect = driver.element_rect(&session_id, element_id).await?;
    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "elementId": element_id,
            "rect": {"x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height}
        }),
        "element rect read",
    ))
}

async fn alert_text(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    let text = driver.alert_text(&session_id).await?;
    Ok(tool_success(
        json!({"ok": true, "sessionId": session_id, "text": text}),
        "alert text read",
    ))
}

async fn alert_accept(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    driver.alert_accept(&session_id).await?;
    Ok(tool_success(
        json!({"ok": true, "sessionId": session_id}),
        "alert accepted",
    ))
}

async fn alert_dismiss(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    driver.alert_dismiss(&session_id).await?;
    Ok(tool_success(
        json!({"ok": true, "sessionId": session_id}),
        "alert dismissed",
    ))
}

async fn alert_wait(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let timeout_ms = arguments
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(10_000)
        .clamp(250, 180_000);

    let driver = driver_from_state(state).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        match driver.alert_text(&session_id).await {
            Ok(text) => {
                return Ok(tool_success(
                    json!({"ok": true, "sessionId": session_id, "text": text}),
                    "alert present",
                ));
            }
            Err(_) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(ToolCallError::new(
                        ToolErrorCode::Timeout,
                        "timeout waiting for alert",
                        json!({"timeoutMs": timeout_ms}),
                    )
                    .into());
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
}

async fn reddit_open_first_post(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let max_candidates = arguments
        .get("maxCandidates")
        .and_then(Value::as_u64)
        .unwrap_or(8)
        .clamp(1, 50) as usize;
    let skip_promoted = arguments
        .get("skipPromoted")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let driver = driver_from_state(state).await?;
    let cells = driver
        .find_elements(&session_id, "accessibility id", "reddit_feed__post__post_cell")
        .await?;

    if cells.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "no Reddit feed post cells found",
            json!({"using": "accessibility id", "value": "reddit_feed__post__post_cell"}),
        )
        .into());
    }

    let mut checked = 0usize;
    let mut skipped = 0usize;

    let promoted_predicate = r#"
        label CONTAINS[c] "Promoted" OR
        label CONTAINS[c] "Sponsored" OR
        label == "Ad" OR
        name CONTAINS[c] "Promoted" OR
        name CONTAINS[c] "Sponsored"
    "#;

    for (idx, cell_id) in cells.iter().take(max_candidates).enumerate() {
        checked += 1;
        let mut is_promoted = false;
        if skip_promoted {
            if let Ok(found) = driver
                .find_elements_from_element(
                    &session_id,
                    cell_id,
                    "ios predicate string",
                    promoted_predicate,
                )
                .await
            {
                is_promoted = !found.is_empty();
            }
        }

        if is_promoted {
            skipped += 1;
            continue;
        }

        driver.click_element(&session_id, cell_id).await?;
        return Ok(tool_success(
            json!({
                "ok": true,
                "sessionId": session_id,
                "openedIndex": idx,
                "elementId": cell_id,
                "checked": checked,
                "skippedPromoted": skipped
            }),
            "opened reddit post",
        ));
    }

    // Fallback: open the first candidate if we could not conclusively skip promoted items.
    let fallback = cells
        .get(0)
        .ok_or_else(|| ToolCallError::new(ToolErrorCode::ElementNotFound, "no candidates", json!({})))?;
    driver.click_element(&session_id, fallback).await?;
    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "openedIndex": 0,
            "elementId": fallback,
            "checked": checked,
            "skippedPromoted": skipped,
            "fallback": true
        }),
        "opened reddit post (fallback)",
    ))
}

#[derive(Debug, Clone)]
struct RedditXmlNode {
    element: String,
    name: Option<String>,
    label: Option<String>,
    value: Option<String>,
    visible: bool,
    x: f64,
    y: f64,
}

fn parse_reddit_nodes(xml: &str) -> Result<Vec<RedditXmlNode>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = false;

    let mut buf = Vec::new();
    let mut out: Vec<RedditXmlNode> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let elem_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if !elem_name.starts_with("XCUIElementType") {
                    buf.clear();
                    continue;
                }

                let mut name: Option<String> = None;
                let mut label: Option<String> = None;
                let mut value: Option<String> = None;
                let mut visible: bool = true;
                let mut x: f64 = 0.0;
                let mut y: f64 = 0.0;

                for attr in e.attributes().with_checks(false) {
                    let attr = attr.context("invalid XML attribute")?;
                    let key = str::from_utf8(attr.key.as_ref()).context("invalid attribute key")?;
                    let val = attr
                        .unescape_value()
                        .context("invalid attribute value")?
                        .into_owned();
                    match key {
                        "name" => name = normalize_text(val),
                        "label" => label = normalize_text(val),
                        "value" => value = normalize_text(val),
                        "visible" => visible = parse_bool(&val, true),
                        "x" => x = val.parse::<f64>().unwrap_or(0.0),
                        "y" => y = val.parse::<f64>().unwrap_or(0.0),
                        _ => {}
                    }
                }

                if !visible {
                    buf.clear();
                    continue;
                }

                if label.is_none() && value.is_none() {
                    buf.clear();
                    continue;
                }

                out.push(RedditXmlNode {
                    element: elem_name,
                    name,
                    label,
                    value,
                    visible,
                    x,
                    y,
                });
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(anyhow!("failed to parse XML: {err}")),
            _ => {}
        }
        buf.clear();
    }

    out.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    Ok(out)
}

fn normalize_text(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn parse_bool(value: &str, default: bool) -> bool {
    match value.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" => true,
        "false" | "0" | "no" => false,
        _ => default,
    }
}

fn normalize_match_key(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

#[derive(Debug, Clone)]
struct NodeQuery {
    element_type: Option<String>,
    name: Option<String>,
    name_prefix: Option<String>,
    label: Option<String>,
    ancestor_name: Option<String>,
    ancestor_type: Option<String>,
    max: Option<usize>,
}

fn parse_node_query(value: Option<&Value>) -> NodeQuery {
    let mut query = NodeQuery {
        element_type: Some("XCUIElementTypeCell".to_string()),
        name: None,
        name_prefix: None,
        label: None,
        ancestor_name: None,
        ancestor_type: None,
        max: None,
    };

    let Some(obj) = value.and_then(Value::as_object) else {
        return query;
    };

    if let Some(value) = obj.get("type").and_then(Value::as_str) {
        if !value.trim().is_empty() {
            query.element_type = Some(value.trim().to_string());
        }
    }
    if let Some(value) = obj.get("name").and_then(Value::as_str) {
        query.name = normalize_text(value.to_string());
    }
    if let Some(value) = obj.get("namePrefix").and_then(Value::as_str) {
        query.name_prefix = normalize_text(value.to_string());
    }
    if let Some(value) = obj.get("label").and_then(Value::as_str) {
        query.label = normalize_text(value.to_string());
    }
    if let Some(value) = obj.get("ancestorName").and_then(Value::as_str) {
        query.ancestor_name = normalize_text(value.to_string());
    }
    if let Some(value) = obj.get("ancestorType").and_then(Value::as_str) {
        query.ancestor_type = normalize_text(value.to_string());
    }
    if let Some(value) = obj.get("max").and_then(Value::as_u64) {
        query.max = Some(value.clamp(1, 100) as usize);
    }

    query
}

#[derive(Debug, Clone)]
struct RowQuery {
    element_type: String,
    name: Option<String>,
    name_prefix: Option<String>,
    ancestor_name: Option<String>,
    ancestor_type: Option<String>,
}

#[derive(Debug, Clone)]
struct PrimaryQuery {
    element_type: String,
    attr: String,
    pick: String,
}

#[derive(Debug, Clone)]
struct TagQuery {
    name_prefix: String,
    pick: String,
    strip_prefix: Option<String>,
    field: String,
}

#[derive(Debug, Clone)]
struct SplitConfig {
    delimiter: String,
    ignore_prefixes: Vec<String>,
    fields: Vec<String>,
    skip_metric_like: bool,
}

#[derive(Debug, Clone)]
struct RowMatch {
    x: f64,
    y: f64,
    raw_label: String,
    fields: Vec<(String, String)>,
    tag_field: Option<String>,
    tag_value: Option<String>,
}

fn parse_row_query(value: Option<&Value>) -> Result<RowQuery> {
    let Some(obj) = value.and_then(Value::as_object) else {
        return Err(anyhow!("row query is required"));
    };
    let element_type = obj
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("XCUIElementTypeCell")
        .to_string();

    Ok(RowQuery {
        element_type,
        name: obj
            .get("name")
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string())),
        name_prefix: obj
            .get("namePrefix")
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string())),
        ancestor_name: obj
            .get("ancestorName")
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string())),
        ancestor_type: obj
            .get("ancestorType")
            .and_then(Value::as_str)
            .and_then(|value| normalize_text(value.to_string())),
    })
}

fn parse_primary_query(value: Option<&Value>) -> Result<PrimaryQuery> {
    let Some(obj) = value.and_then(Value::as_object) else {
        return Err(anyhow!("primary query is required"));
    };
    let element_type = obj
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("XCUIElementTypeButton")
        .to_string();
    let attr = obj
        .get("attr")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("label")
        .to_string();
    let pick = obj
        .get("pick")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("longest")
        .to_string();

    Ok(PrimaryQuery {
        element_type,
        attr,
        pick,
    })
}

fn parse_tag_query(value: Option<&Value>) -> Option<TagQuery> {
    let obj = value.and_then(Value::as_object)?;
    let name_prefix = obj
        .get("namePrefix")
        .and_then(Value::as_str)
        .and_then(|value| normalize_text(value.to_string()))?;
    let pick = obj
        .get("pick")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("last")
        .to_string();
    let strip_prefix = obj
        .get("stripPrefix")
        .and_then(Value::as_str)
        .and_then(|value| normalize_text(value.to_string()));
    let field = obj
        .get("field")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("tag")
        .to_string();
    Some(TagQuery {
        name_prefix,
        pick,
        strip_prefix,
        field,
    })
}

fn parse_split_config(value: Option<&Value>) -> SplitConfig {
    let mut cfg = SplitConfig {
        delimiter: ",".to_string(),
        ignore_prefixes: Vec::new(),
        fields: vec!["name".to_string(), "subtitle".to_string()],
        skip_metric_like: true,
    };

    let Some(obj) = value.and_then(Value::as_object) else {
        return cfg;
    };

    if let Some(delim) = obj.get("delimiter").and_then(Value::as_str) {
        if !delim.trim().is_empty() {
            cfg.delimiter = delim.to_string();
        }
    }
    if let Some(values) = obj.get("ignorePrefixes").and_then(Value::as_array) {
        cfg.ignore_prefixes = values
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|value| normalize_text(value.to_string()))
            .collect();
    }
    if let Some(values) = obj.get("fields").and_then(Value::as_array) {
        let fields: Vec<String> = values
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|value| normalize_text(value.to_string()))
            .collect();
        if !fields.is_empty() {
            cfg.fields = fields;
        }
    }
    if let Some(value) = obj.get("skipMetricLike").and_then(Value::as_bool) {
        cfg.skip_metric_like = value;
    }

    cfg
}

fn resolve_prefixes_for_typeahead(arguments: &Value) -> Result<Vec<String>> {
    if let Some(values) = arguments.get("prefixes").and_then(Value::as_array) {
        let mut out: Vec<String> = values
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|value| normalize_text(value.to_string()))
            .collect();
        out.retain(|value| !value.is_empty());
        if !out.is_empty() {
            return Ok(out);
        }
    }

    let Some(query) = arguments.get("query").and_then(Value::as_str) else {
        return Err(anyhow!("query or prefixes[] is required"));
    };
    let query = query.trim();
    if query.is_empty() {
        return Err(anyhow!("query or prefixes[] is required"));
    }

    let mut prefixes = Vec::new();
    let mut cur = String::new();
    for ch in query.chars() {
        cur.push(ch);
        if let Some(normalized) = normalize_text(cur.clone()) {
            prefixes.push(normalized);
        }
    }
    if prefixes.is_empty() {
        prefixes.push(query.to_string());
    }
    prefixes.dedup();
    Ok(prefixes)
}

async fn type_into_field(
    state: &AppState,
    driver: &WebDriverClient,
    session_id: &str,
    field: &Value,
    prefix: &str,
    typing_mode: &str,
) -> Result<()> {
    let resolved = resolve_target(
        state,
        &json!({
            "sessionId": session_id,
            "target": field
        }),
    )
    .await?
    .ok_or_else(|| anyhow!("unable to resolve field target"))?;
    let ids = driver
        .find_elements(session_id, &resolved.using, &resolved.value)
        .await?;
    let element_id = ids
        .get(resolved.index)
        .ok_or_else(|| anyhow!("no field element found for typeahead"))?;
    let _ = driver.click_element(session_id, element_id).await;
    let _ = driver.clear_element(session_id, element_id).await;
    if let Ok(clear_ids) = driver
        .find_elements(session_id, "accessibility id", "Clear text")
        .await
    {
        if let Some(clear_id) = clear_ids.first() {
            let _ = driver.click_element(session_id, clear_id).await;
        }
    }

    if typing_mode == "char-by-char" {
        for ch in prefix.chars() {
            let text = ch.to_string();
            driver.type_element(session_id, element_id, &text).await?;
            tokio::time::sleep(Duration::from_millis(80)).await;
        }
    } else {
        driver.type_element(session_id, element_id, prefix).await?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct TextNodeMatch {
    text: String,
    x: f64,
    y: f64,
}

fn extract_suggestion_texts(
    source: &str,
    query: &NodeQuery,
    limit: usize,
) -> Vec<Value> {
    let mut nodes = extract_nodes_from_source(source, query);
    nodes.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut seen = HashSet::<String>::new();
    let mut out = Vec::new();
    for node in nodes {
        let key = normalize_match_key(&node.text);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        out.push(json!({"text": node.text, "position": out.len() + 1}));
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn extract_nodes_from_source(source: &str, query: &NodeQuery) -> Vec<TextNodeMatch> {
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    let mut stack: Vec<(String, Option<String>)> = Vec::new();
    let mut out = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let elem_type = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let name = attr_text(&e, "name");
                if node_matches(&e, &elem_type, query, &stack) {
                    if let Some(text) = extract_preferred_text(&e) {
                        let (x, y) = (attr_f64(&e, "x").unwrap_or(0.0), attr_f64(&e, "y").unwrap_or(0.0));
                        out.push(TextNodeMatch { text, x, y });
                        if let Some(max) = query.max {
                            if out.len() >= max {
                                buf.clear();
                                break;
                            }
                        }
                    }
                }
                stack.push((elem_type, name));
            }
            Ok(Event::Empty(e)) => {
                let elem_type = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if node_matches(&e, &elem_type, query, &stack) {
                    if let Some(text) = extract_preferred_text(&e) {
                        let (x, y) = (attr_f64(&e, "x").unwrap_or(0.0), attr_f64(&e, "y").unwrap_or(0.0));
                        out.push(TextNodeMatch { text, x, y });
                        if let Some(max) = query.max {
                            if out.len() >= max {
                                buf.clear();
                                break;
                            }
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                stack.pop();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    out
}

fn node_matches(
    elem: &quick_xml::events::BytesStart<'_>,
    elem_type: &str,
    query: &NodeQuery,
    ancestors: &[(String, Option<String>)],
) -> bool {
    if let Some(want_type) = &query.element_type {
        if elem_type != want_type {
            return false;
        }
    }

    if let Some(want_name) = &query.name {
        if attr_text(elem, "name").as_deref() != Some(want_name.as_str()) {
            return false;
        }
    }
    if let Some(prefix) = &query.name_prefix {
        let name = attr_text(elem, "name").unwrap_or_default();
        if !name.starts_with(prefix) {
            return false;
        }
    }
    if let Some(label) = &query.label {
        if attr_text(elem, "label").as_deref() != Some(label.as_str()) {
            return false;
        }
    }

    if query.ancestor_name.is_none() && query.ancestor_type.is_none() {
        return true;
    }

    ancestors.iter().any(|(ancestor_type, ancestor_name)| {
        if let Some(want_name) = &query.ancestor_name {
            if ancestor_name.as_deref() != Some(want_name.as_str()) {
                return false;
            }
        }
        if let Some(want_type) = &query.ancestor_type {
            if ancestor_type != want_type {
                return false;
            }
        }
        true
    })
}

fn extract_rows_from_source(
    source: &str,
    row_query: &RowQuery,
    primary_query: &PrimaryQuery,
    tag_query: Option<&TagQuery>,
    split_cfg: &SplitConfig,
) -> Vec<RowMatch> {
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    let mut stack: Vec<(String, Option<String>)> = Vec::new();
    let mut current: Option<(usize, f64, f64, Vec<String>, Vec<String>)> = None;
    let mut out = Vec::new();

    let mut depth = 0usize;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                depth += 1;
                let elem_type = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let name = attr_text(&e, "name");

                if current.is_none()
                    && element_matches_row(&e, &elem_type, row_query, &stack)
                {
                    current = Some((
                        depth,
                        attr_f64(&e, "x").unwrap_or(0.0),
                        attr_f64(&e, "y").unwrap_or(0.0),
                        Vec::new(),
                        Vec::new(),
                    ));
                }

                if let Some((_row_depth, _x, _y, labels, tags)) = current.as_mut() {
                    collect_primary_label(&elem_type, &e, primary_query, labels);
                    collect_tag_value(&e, tag_query, tags);
                }

                stack.push((elem_type, name));
            }
            Ok(Event::Empty(e)) => {
                let elem_type = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if current.is_none()
                    && element_matches_row(&e, &elem_type, row_query, &stack)
                {
                    let row = finalize_row(
                        attr_f64(&e, "x").unwrap_or(0.0),
                        attr_f64(&e, "y").unwrap_or(0.0),
                        Vec::new(),
                        Vec::new(),
                        primary_query,
                        tag_query,
                        split_cfg,
                    );
                    if let Some(row) = row {
                        out.push(row);
                    }
                } else if let Some((_row_depth, _x, _y, labels, tags)) = current.as_mut() {
                    collect_primary_label(&elem_type, &e, primary_query, labels);
                    collect_tag_value(&e, tag_query, tags);
                }
            }
            Ok(Event::End(_)) => {
                if let Some((row_depth, x, y, labels, tags)) = current.take() {
                    if row_depth == depth {
                        if let Some(row) = finalize_row(x, y, labels, tags, primary_query, tag_query, split_cfg) {
                            out.push(row);
                        }
                    } else {
                        current = Some((row_depth, x, y, labels, tags));
                    }
                }
                stack.pop();
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    out
}

fn element_matches_row(
    elem: &quick_xml::events::BytesStart<'_>,
    elem_type: &str,
    query: &RowQuery,
    ancestors: &[(String, Option<String>)],
) -> bool {
    if elem_type != query.element_type {
        return false;
    }
    if let Some(want) = &query.name {
        if attr_text(elem, "name").as_deref() != Some(want.as_str()) {
            return false;
        }
    }
    if let Some(prefix) = &query.name_prefix {
        let name = attr_text(elem, "name").unwrap_or_default();
        if !name.starts_with(prefix) {
            return false;
        }
    }
    if query.ancestor_name.is_none() && query.ancestor_type.is_none() {
        return true;
    }
    ancestors.iter().any(|(ancestor_type, ancestor_name)| {
        if let Some(want_name) = &query.ancestor_name {
            if ancestor_name.as_deref() != Some(want_name.as_str()) {
                return false;
            }
        }
        if let Some(want_type) = &query.ancestor_type {
            if ancestor_type != want_type {
                return false;
            }
        }
        true
    })
}

fn collect_primary_label(
    elem_type: &str,
    elem: &quick_xml::events::BytesStart<'_>,
    query: &PrimaryQuery,
    labels: &mut Vec<String>,
) {
    if elem_type != query.element_type {
        return;
    }
    if let Some(value) = attr_text(elem, &query.attr) {
        labels.push(value);
    }
}

fn collect_tag_value(
    elem: &quick_xml::events::BytesStart<'_>,
    query: Option<&TagQuery>,
    tags: &mut Vec<String>,
) {
    let Some(query) = query else {
        return;
    };
    let Some(name) = attr_text(elem, "name") else {
        return;
    };
    if let Some(stripped) = name.strip_prefix(&query.name_prefix) {
        let cleaned = stripped.trim();
        if !cleaned.is_empty() {
            tags.push(cleaned.to_string());
        }
    }
}

fn finalize_row(
    x: f64,
    y: f64,
    labels: Vec<String>,
    tags: Vec<String>,
    primary_query: &PrimaryQuery,
    tag_query: Option<&TagQuery>,
    split_cfg: &SplitConfig,
) -> Option<RowMatch> {
    let raw_label = if primary_query.pick == "first" {
        labels.first().cloned().unwrap_or_default()
    } else {
        labels
            .into_iter()
            .max_by_key(|value| value.len())
            .unwrap_or_default()
    };
    if raw_label.is_empty() {
        return None;
    }

    let mut parts: Vec<String> = raw_label
        .split(&split_cfg.delimiter)
        .filter_map(|value| normalize_text(value.to_string()))
        .collect();
    if let Some(first) = parts.first() {
        if split_cfg
            .ignore_prefixes
            .iter()
            .any(|prefix| prefix.eq_ignore_ascii_case(first))
        {
            parts.remove(0);
        }
    }
    if split_cfg.skip_metric_like {
        parts.retain(|part| !metric_like(part));
    }

    let mut fields = Vec::new();
    for (idx, field_name) in split_cfg.fields.iter().enumerate() {
        let value = parts.get(idx).cloned().unwrap_or_default();
        fields.push((field_name.clone(), value));
    }

    let (tag_field, tag_value) = if let Some(tag_query) = tag_query {
        let selected = if tag_query.pick == "first" {
            tags.first().cloned()
        } else {
            tags.last().cloned()
        };
        let cleaned = selected.map(|value| {
            if let Some(prefix) = &tag_query.strip_prefix {
                value.strip_prefix(prefix).unwrap_or(&value).trim().to_string()
            } else {
                value
            }
        });
        (Some(tag_query.field.clone()), cleaned)
    } else {
        (None, None)
    };

    Some(RowMatch {
        x,
        y,
        raw_label,
        fields,
        tag_field,
        tag_value,
    })
}

fn metric_like(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.contains("star")
        || lower.contains("rating")
        || lower.ends_with("ratings")
        || lower.ends_with("rating")
        || lower.ends_with("reviews")
}

fn attr_text(elem: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    for attr in elem.attributes().with_checks(false) {
        let Ok(attr) = attr else {
            continue;
        };
        let Ok(name) = std::str::from_utf8(attr.key.as_ref()) else {
            continue;
        };
        if name != key {
            continue;
        }
        let Ok(raw) = attr.unescape_value() else {
            continue;
        };
        return normalize_text(raw.into_owned());
    }
    None
}

fn attr_f64(elem: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<f64> {
    attr_text(elem, key).and_then(|value| value.parse::<f64>().ok())
}

fn extract_preferred_text(elem: &quick_xml::events::BytesStart<'_>) -> Option<String> {
    attr_text(elem, "label")
        .or_else(|| attr_text(elem, "name"))
        .or_else(|| attr_text(elem, "value"))
}

async fn reddit_extract_post(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let max_comments = arguments
        .get("maxComments")
        .and_then(Value::as_u64)
        .unwrap_or(3)
        .clamp(0, 10) as usize;
    let max_raw_lines = arguments
        .get("maxRawLines")
        .and_then(Value::as_u64)
        .unwrap_or(80)
        .clamp(10, 300) as usize;

    let driver = driver_from_state(state).await?;
    let source = driver.page_source(&session_id).await?;
    let nodes = parse_reddit_nodes(&source)?;

    let mut raw_lines: Vec<String> = Vec::new();
    let mut seen = HashSet::<String>::new();
    for node in &nodes {
        let text = node
            .label
            .clone()
            .or_else(|| node.value.clone())
            .unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        if seen.insert(text.clone()) {
            raw_lines.push(text);
            if raw_lines.len() >= max_raw_lines {
                break;
            }
        }
    }

    let title = nodes
        .iter()
        .find(|n| {
            n.name.as_deref() == Some("reddit_feed__post__title_text")
                || n.name
                    .as_deref()
                    .map(|name| name.contains("title_text"))
                    .unwrap_or(false)
        })
        .and_then(|n| n.label.clone().or_else(|| n.value.clone()));

    let subreddit = nodes
        .iter()
        .find(|n| {
            n.label
                .as_deref()
                .map(|t| t.starts_with("r/"))
                .unwrap_or(false)
                || n.name
                    .as_deref()
                    .map(|name| name.contains("subreddit"))
                    .unwrap_or(false)
        })
        .and_then(|n| n.label.clone().or_else(|| n.value.clone()));

    let author = nodes
        .iter()
        .find(|n| {
            n.label
                .as_deref()
                .map(|t| t.starts_with("u/"))
                .unwrap_or(false)
                || n.name
                    .as_deref()
                    .map(|name| name.contains("author"))
                    .unwrap_or(false)
        })
        .and_then(|n| n.label.clone().or_else(|| n.value.clone()));

    let body = nodes
        .iter()
        .find(|n| {
            n.name
                .as_deref()
                .map(|name| {
                    name.contains("post_body")
                        || name.contains("selftext")
                        || (name.contains("body") && !name.contains("comment"))
                })
                .unwrap_or(false)
                || n.element.ends_with("TextView")
        })
        .and_then(|n| n.label.clone().or_else(|| n.value.clone()))
        .filter(|text| title.as_ref().map(|t| t != text).unwrap_or(true));

    let mut comments: Vec<String> = Vec::new();
    let mut comment_seen = HashSet::<String>::new();
    if max_comments > 0 {
        for node in &nodes {
            let Some(name) = node.name.as_deref() else {
                continue;
            };
            if !name.contains("comment") || name.contains("composer") {
                continue;
            }
            let text = node
                .label
                .clone()
                .or_else(|| node.value.clone())
                .unwrap_or_default();
            if text.is_empty() {
                continue;
            }
            if comment_seen.insert(text.clone()) {
                comments.push(text);
                if comments.len() >= max_comments {
                    break;
                }
            }
        }
    }

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "title": title,
            "subreddit": subreddit,
            "author": author,
            "body": body,
            "topComments": comments,
            "rawLines": raw_lines
        }),
        "reddit post extracted",
    ))
}

async fn session_delete(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let stop_appium = arguments
        .get("stopAppium")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let shutdown_wda = arguments
        .get("shutdownWDA")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let snapshot = state.snapshot().await;
    let wda_port = state
        .last_wda_local_port()
        .await
        .unwrap_or(DEFAULT_WDA_LOCAL_PORT);
    let driver = driver_from_state(state).await?;
    driver.delete_session(&session_id).await?;

    let wda_shutdown_ok = if shutdown_wda {
        shutdown_wda_on_port(wda_port).await.unwrap_or(false)
    } else {
        false
    };

    state.clear_session().await;

    let mut stopped_appium = false;
    if stop_appium {
        match snapshot.appium_source {
            Some(AppiumSource::Spawned) => {
                state.shutdown_spawned_appium().await;
                stopped_appium = true;
            }
            Some(AppiumSource::Env) => {
                if let Some(base_url) = snapshot.appium_base_url.as_deref() {
                    stopped_appium = stop_local_env_appium(base_url).await.unwrap_or(false);
                }
                state.clear_appium_metadata().await;
            }
            None => {}
        }
    }

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "stopAppium": stop_appium,
            "stoppedAppium": stopped_appium,
            "shutdownWDA": shutdown_wda,
            "wdaLocalPort": wda_port,
            "wdaShutdownOk": wda_shutdown_ok
        }),
        "session deleted",
    ))
}

async fn session_info(state: &AppState) -> Result<Value> {
    let snapshot = state.snapshot().await;
    Ok(tool_success(
        json!({
            "ok": true,
            "appiumBaseUrl": snapshot.appium_base_url,
            "appiumPid": snapshot.appium_pid,
            "session": snapshot.session
        }),
        "session info",
    ))
}

async fn wda_shutdown(state: &AppState, arguments: &Value) -> Result<Value> {
    let port_from_args = arguments
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .filter(|v| *v > 0);

    let port = match port_from_args {
        Some(port) => port,
        None => state
            .last_wda_local_port()
            .await
            .unwrap_or(DEFAULT_WDA_LOCAL_PORT),
    };

    let ok = shutdown_wda_on_port(port).await.unwrap_or(false);

    Ok(tool_success(
        json!({
            "ok": true,
            "port": port,
            "shutdownOk": ok
        }),
        "wda shutdown attempted",
    ))
}

async fn web_goto(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let url = required_str(arguments, "url")?;
    let driver = driver_from_state(state).await?;
    driver.goto_url(&session_id, url).await?;

    Ok(tool_success(
        json!({"ok": true, "sessionId": session_id, "url": url}),
        "navigation complete",
    ))
}

async fn web_wait_css(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let selector = required_str(arguments, "selector")?;
    let index = arguments
        .get("index")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .clamp(0, 10_000) as usize;
    let require_unique = arguments
        .get("requireUnique")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let timeout_ms = arguments
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(10_000)
        .clamp(500, 120_000);

    let driver = driver_from_state(state).await?;
    let element_id = wait_for_selector(
        &driver,
        &session_id,
        selector,
        index,
        require_unique,
        Duration::from_millis(timeout_ms),
    )
    .await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "selector": selector,
            "index": index,
            "elementId": element_id
        }),
        "selector found",
    ))
}

async fn web_click_css(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let selector = required_str(arguments, "selector")?;
    let index = arguments
        .get("index")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .clamp(0, 10_000) as usize;
    let require_unique = arguments
        .get("requireUnique")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let driver = driver_from_state(state).await?;

    let ids = driver.find_elements_css(&session_id, selector).await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!("no elements found for selector '{selector}'"),
            json!({"selector": selector}),
        )
        .into());
    }
    if require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            format!("expected exactly one match for selector '{selector}', got {}", ids.len()),
            json!({"selector": selector, "matchCount": ids.len()}),
        )
        .into());
    }
    let element_id = ids.get(index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!("no element at index {index} for selector '{selector}' (found {})", ids.len()),
            json!({"selector": selector, "index": index, "matchCount": ids.len()}),
        )
    })?;
    driver.click_element(&session_id, element_id).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "selector": selector,
            "index": index,
            "elementId": element_id
        }),
        "click complete",
    ))
}

async fn web_type_css(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let selector = required_str(arguments, "selector")?;
    let index = arguments
        .get("index")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .clamp(0, 10_000) as usize;
    let require_unique = arguments
        .get("requireUnique")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let text = required_str(arguments, "text")?;
    let clear_first = arguments
        .get("clearFirst")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let driver = driver_from_state(state).await?;
    let ids = driver.find_elements_css(&session_id, selector).await?;
    if ids.is_empty() {
        return Err(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!("no elements found for selector '{selector}'"),
            json!({"selector": selector}),
        )
        .into());
    }
    if require_unique && ids.len() != 1 {
        return Err(ToolCallError::new(
            ToolErrorCode::AmbiguousMatch,
            format!("expected exactly one match for selector '{selector}', got {}", ids.len()),
            json!({"selector": selector, "matchCount": ids.len()}),
        )
        .into());
    }
    let element_id = ids.get(index).ok_or_else(|| {
        ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            format!("no element at index {index} for selector '{selector}' (found {})", ids.len()),
            json!({"selector": selector, "index": index, "matchCount": ids.len()}),
        )
    })?;

    if clear_first {
        let _ = driver.clear_element(&session_id, element_id).await;
    }
    driver.type_element(&session_id, element_id, text).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "selector": selector,
            "index": index,
            "elementId": element_id,
            "typedLength": text.chars().count()
        }),
        "type complete",
    ))
}

async fn web_press_key(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let key = arguments
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or("Enter")
        .to_lowercase();

    let driver = driver_from_state(state).await?;
    match key.as_str() {
        "enter" | "return" | "search" => driver.press_enter(&session_id).await?,
        _ => bail!("unsupported key '{key}', supported: Enter|Return|Search"),
    }

    Ok(tool_success(
        json!({"ok": true, "sessionId": session_id, "key": key}),
        "key press complete",
    ))
}

async fn web_page_source(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    let source = driver.page_source(&session_id).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "sessionId": session_id,
            "length": source.len(),
            "source": source
        }),
        "page source captured",
    ))
}

async fn web_screenshot(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let driver = driver_from_state(state).await?;
    let data = driver.screenshot(&session_id).await?;

    Ok(tool_success_with_content(
        json!({
            "ok": true,
            "sessionId": session_id,
            "mimeType": "image/png",
            "bytesBase64": data.len(),
            "data": data
        }),
        vec![
            json!({"type": "text", "text": "screenshot captured"}),
            json!({"type": "image", "mimeType": "image/png", "data": data}),
        ],
    ))
}

async fn web_eval_js(state: &AppState, arguments: &Value) -> Result<Value> {
    let session_id = resolve_session_id(state, arguments).await?;
    let script = required_str(arguments, "script")?;
    let args = arguments.get("args").cloned().unwrap_or_else(|| json!([]));

    if !args.is_array() {
        bail!("args must be an array for ios.web.eval_js");
    }

    let driver = driver_from_state(state).await?;
    let response = driver.execute_script(&session_id, script, args).await?;

    Ok(tool_success(
        json!({
            "ok": true,
            "risk": "high",
            "sessionId": session_id,
            "result": response.get("value").cloned().unwrap_or(Value::Null)
        }),
        "script executed",
    ))
}

async fn workflow_list() -> Result<Value> {
    let flows = workflows::list_workflows();
    Ok(tool_success(json!({ "workflows": flows }), "workflow list"))
}

async fn workflow_run(state: &AppState, arguments: &Value) -> Result<Value> {
    let name = required_str(arguments, "name")?;
    let commit = arguments.get("commit").and_then(Value::as_bool).unwrap_or(false);
    let close_on_finish = arguments
        .get("closeOnFinish")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let stop_appium_on_finish = arguments
        .get("stopAppiumOnFinish")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let output_result = if let Some(def) = workflows::load_file_workflow(name) {
        if let Some(steps) = def.steps {
            let vars = build_workflow_vars(arguments);
            run_steps(state, &steps, commit, &vars, def.output.as_ref()).await
        } else {
            bail!("workflow '{name}' has no executable steps")
        }
    } else {
        bail!("unknown workflow '{name}' (no JSON workflow found)")
    };

    let output = match output_result {
        Ok(output) => output,
        Err(err) => {
            let artifacts = capture_failure_artifacts(state)
                .await
                .unwrap_or_else(|_| json!({}));

            if stop_appium_on_finish {
                let _ = worker_shutdown(state, &json!({"stopAppium": true, "shutdownWDA": true})).await;
            } else if close_on_finish {
                let _ = session_delete(state, &json!({"stopAppium": false, "shutdownWDA": true})).await;
            }

            let message = format!("workflow '{name}' failed: {err:#}");
            let lowered = message.to_lowercase();
            let code = if lowered.contains("device was not, or could not be, unlocked")
                || lowered.contains("could not be unlocked")
                || lowered.contains("bserrorcodedescription=locked")
                || lowered.contains(" for reason: locked")
            {
                ToolErrorCode::DeviceLocked
            } else if lowered.contains("timeout") {
                ToolErrorCode::Timeout
            } else {
                ToolErrorCode::ActionFailed
            };

            return Err(ToolCallError::new(
                code,
                message,
                json!({
                    "workflow": name,
                    "artifacts": artifacts
                }),
            )
            .into());
        }
    };

    let screenshot_block = output
        .get("screenshot")
        .and_then(|value| value.get("data").and_then(Value::as_str))
        .filter(|data| !data.trim().is_empty())
        .map(|data| {
            json!({
                "type": "image",
                "mimeType": output.get("screenshot").and_then(|v| v.get("mimeType")).and_then(Value::as_str).unwrap_or("image/png"),
                "data": data
            })
        })
        .or_else(|| {
            output
                .get("trace")
                .and_then(Value::as_array)
                .and_then(|trace| {
                    trace.iter().rev().find_map(|entry| {
                        let result = entry.get("result")?;
                        let content = result.get("content")?.as_array()?;
                        content.iter().find_map(|block| {
                            let typ = block.get("type")?.as_str()?;
                            if typ != "image" {
                                return None;
                            }
                            let data = block.get("data")?.as_str()?;
                            if data.trim().is_empty() {
                                return None;
                            }
                            Some(block.clone())
                        })
                    })
                })
        })
        .unwrap_or_else(|| json!({"type": "text", "text": "no screenshot"}));

    let content = vec![
        json!({"type": "text", "text": format!("workflow '{name}' completed")}),
        screenshot_block,
    ];

    if stop_appium_on_finish {
        let _ = worker_shutdown(state, &json!({"stopAppium": true, "shutdownWDA": true})).await;
    } else if close_on_finish {
        let _ = session_delete(state, &json!({"stopAppium": false, "shutdownWDA": true})).await;
    }

    Ok(tool_success_with_content(output, content))
}

fn build_workflow_vars(arguments: &Value) -> Value {
    let mut vars = serde_json::Map::new();

    if let Some(obj) = arguments.get("args").and_then(Value::as_object) {
        for (k, v) in obj {
            vars.insert(k.clone(), v.clone());
        }
    }

    if let Some(obj) = arguments.get("session").and_then(Value::as_object) {
        for (k, v) in obj {
            if k == "signing" {
                continue;
            }
            vars.insert(k.clone(), v.clone());
        }
        if let Some(signing) = obj.get("signing").and_then(Value::as_object) {
            for (k, v) in signing {
                vars.insert(k.clone(), v.clone());
            }
        }
    }

    vars.entry("showXcodeLog".to_string())
        .or_insert_with(|| json!(false));
    vars.entry("allowProvisioningUpdates".to_string())
        .or_insert_with(|| json!(false));
    vars.entry("allowProvisioningDeviceRegistration".to_string())
        .or_insert_with(|| json!(false));
    vars.entry("sessionCreateTimeoutMs".to_string())
        .or_insert_with(|| json!(600_000));
    vars.entry("wdaLaunchTimeoutMs".to_string())
        .or_insert_with(|| json!(240_000));
    vars.entry("wdaConnectionTimeoutMs".to_string())
        .or_insert_with(|| json!(120_000));

    vars.entry("xcodeOrgId".to_string())
        .or_insert_with(|| json!(""));
    vars.entry("xcodeSigningId".to_string())
        .or_insert_with(|| json!(""));
    vars.entry("updatedWDABundleId".to_string())
        .or_insert_with(|| json!(""));

    Value::Object(vars)
}

async fn script_run(state: &AppState, arguments: &Value) -> Result<Value> {
    let steps = arguments
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("steps must be an array"))?;
    let commit = arguments.get("commit").and_then(Value::as_bool).unwrap_or(false);
    let vars = arguments.get("vars").cloned().unwrap_or_else(|| json!({}));
    let close_on_finish = arguments
        .get("closeOnFinish")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let stop_appium_on_finish = arguments
        .get("stopAppiumOnFinish")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let result = run_steps(state, steps, commit, &vars, None).await?;

    if stop_appium_on_finish {
        let _ = worker_shutdown(state, &json!({"stopAppium": true})).await;
    } else if close_on_finish {
        let _ = session_delete(state, &json!({"stopAppium": false})).await;
    }

    Ok(tool_success(result, "script complete"))
}

async fn run_steps(
    state: &AppState,
    steps: &[Value],
    commit: bool,
    vars: &Value,
    output_template: Option<&Value>,
) -> Result<Value> {
    let mut trace: Vec<Value> = Vec::with_capacity(steps.len());
    let mut vars = vars.clone();
    ensure_workflow_steps_var(&mut vars);

    for (idx, step) in steps.iter().enumerate() {
        let Some(obj) = step.as_object() else {
            bail!("step {idx} must be an object");
        };

        let step_id = obj
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);

        let tool = obj
            .get("tool")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow!("step {idx} missing tool"))?;

        if tool == "ios.script.run" || tool == "ios.workflow.run" {
            bail!("step {idx} tool '{tool}' is not allowed");
        }

        let requires_commit = obj
            .get("requiresCommit")
            .and_then(Value::as_bool)
            .or_else(|| obj.get("requires_commit").and_then(Value::as_bool))
            .unwrap_or(false);
        if requires_commit && !commit {
            let message = format!("step {idx} requires commit=true (tool={tool})");
            trace.push(json!({
                "step": idx + 1,
                "stepId": step_id.clone(),
                "tool": tool,
                "attempt": 0,
                "ok": false,
                "durationMs": 0,
                "error": message,
                "errorCode": ToolErrorCode::CommitRequired.as_str(),
                "errorDetails": {"tool": tool, "step": idx + 1}
            }));
            return Ok(json!({
                "ok": false,
                "failedStep": idx + 1,
                "error": message,
                "errorCode": ToolErrorCode::CommitRequired.as_str(),
                "trace": trace
            }));
        }

        let retries = obj
            .get("retries")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, 10) as u32;
        let timeout_ms = obj
            .get("timeoutMs")
            .and_then(Value::as_u64)
            .or_else(|| obj.get("timeout_ms").and_then(Value::as_u64))
            .unwrap_or(120_000)
            .clamp(250, 600_000);

        let raw_args = obj
            .get("arguments")
            .cloned()
            .or_else(|| obj.get("args").cloned())
            .unwrap_or_else(|| json!({}));
        let args = substitute_vars(raw_args, &vars);

        let started = tokio::time::Instant::now();
        let mut last_err: Option<anyhow::Error> = None;
        let mut attempt: u32 = 0;

        loop {
            attempt += 1;
            let call_fut = handle_tool_call(state, tool, args.clone());
            let call =
                tokio::time::timeout(Duration::from_millis(timeout_ms), Box::pin(call_fut)).await;

            match call {
                Ok(Ok(result)) => {
                    if let Some(save_as) = step_save_as(obj) {
                        let stored = result
                            .get("structuredContent")
                            .cloned()
                            .unwrap_or_else(|| result.clone());
                        store_step_output(&mut vars, &save_as, stored);
                    }
                    trace.push(json!({
                        "step": idx + 1,
                        "stepId": step_id.clone(),
                        "tool": tool,
                        "attempt": attempt,
                        "ok": true,
                        "durationMs": started.elapsed().as_millis(),
                        "result": result
                    }));
                    break;
                }
                Ok(Err(err)) => {
                    last_err = Some(err);
                }
                Err(_) => {
                    last_err = Some(anyhow!("timeout after {timeout_ms}ms"));
                }
            }

            if attempt > retries + 1 {
                let err = last_err.unwrap_or_else(|| anyhow!("unknown error"));
                let artifacts = capture_failure_artifacts(state).await.unwrap_or_else(|_| json!({}));
                let tool_error = tool_error_from_anyhow(&err, tool);
                let structured = tool_error
                    .get("structuredContent")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                let error_code = structured
                    .get("errorCode")
                    .cloned()
                    .unwrap_or(Value::Null);
                let error_message = structured
                    .get("error")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("{err:#}"));
                let error_details = structured
                    .get("details")
                    .cloned()
                    .unwrap_or_else(|| json!({ "tool": tool }));
                trace.push(json!({
                    "step": idx + 1,
                    "stepId": step_id.clone(),
                    "tool": tool,
                    "attempt": attempt,
                    "ok": false,
                    "durationMs": started.elapsed().as_millis(),
                    "error": error_message,
                    "errorCode": error_code,
                    "errorDetails": error_details
                }));
                return Ok(json!({
                    "ok": false,
                    "failedStep": idx + 1,
                    "error": error_message,
                    "errorCode": error_code,
                    "artifacts": artifacts,
                    "trace": trace
                }));
            }

            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    let mut output = if let Some(template) = output_template {
        render_workflow_output(template, &vars, steps.len(), trace.clone())
    } else {
        json!({
            "ok": true,
            "steps": steps.len(),
            "trace": trace
        })
    };

    if let Some(obj) = output.as_object_mut() {
        obj.entry("ok".to_string()).or_insert_with(|| json!(true));
    }

    Ok(output)
}

async fn capture_failure_artifacts(state: &AppState) -> Result<Value> {
    let Some(session) = state.active_session().await else {
        return Ok(json!({}));
    };

    let driver = driver_from_state(state).await?;
    let mut out = serde_json::Map::new();

    if let Ok(png_b64) = driver.screenshot(&session.session_id).await {
        out.insert(
            "screenshot".to_string(),
            json!({"mimeType": "image/png", "data": png_b64}),
        );
    }

    if let Ok(source) = driver.page_source(&session.session_id).await {
        let truncated = source.len() > 50_000;
        let slice = if truncated {
            source.chars().take(50_000).collect::<String>()
        } else {
            source
        };
        out.insert(
            "uiSource".to_string(),
            json!({"length": slice.len(), "truncated": truncated, "source": slice}),
        );
    }

    Ok(Value::Object(out))
}

async fn util_rank_by_name(arguments: &Value) -> Result<Value> {
    let items = arguments
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("items must be an array"))?;
    let field = arguments
        .get("field")
        .and_then(Value::as_str)
        .unwrap_or("name");
    let target = required_str(arguments, "target")?;
    let want = normalize_match_key(target);

    let mut rank: Option<usize> = None;
    for (idx, item) in items.iter().enumerate() {
        let Some(value) = item.get(field).and_then(Value::as_str) else {
            continue;
        };
        let candidate = normalize_match_key(value);
        if candidate == want {
            rank = Some(idx + 1);
            break;
        }
    }
    if rank.is_none() && !want.is_empty() {
        for (idx, item) in items.iter().enumerate() {
            let Some(value) = item.get(field).and_then(Value::as_str) else {
                continue;
            };
            let candidate = normalize_match_key(value);
            if candidate.contains(&want) || want.contains(&candidate) {
                rank = Some(idx + 1);
                break;
            }
        }
    }

    Ok(tool_success(
        json!({ "ok": true, "rank": rank }),
        "rank computed",
    ))
}

async fn util_list_length(arguments: &Value) -> Result<Value> {
    let list = arguments
        .get("list")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("list must be an array"))?;

    Ok(tool_success(
        json!({ "ok": true, "count": list.len() }),
        "length computed",
    ))
}

fn substitute_vars(value: Value, vars: &Value) -> Value {
    match value {
        Value::String(s) => {
            if let Some(exact) = substitute_exact_value(&s, vars) {
                exact
            } else {
                Value::String(substitute_string(&s, vars))
            }
        }
        Value::Array(items) => Value::Array(items.into_iter().map(|v| substitute_vars(v, vars)).collect()),
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k, substitute_vars(v, vars));
            }
            Value::Object(out)
        }
        other => other,
    }
}

fn ensure_workflow_steps_var(vars: &mut Value) {
    let Some(map) = vars.as_object_mut() else {
        *vars = json!({ "steps": {} });
        return;
    };
    match map.get_mut("steps") {
        Some(Value::Object(_)) => {}
        _ => {
            map.insert("steps".to_string(), json!({}));
        }
    }
}

fn step_save_as(step: &serde_json::Map<String, Value>) -> Option<String> {
    step.get("saveAs")
        .and_then(Value::as_str)
        .or_else(|| step.get("save_as").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn store_step_output(vars: &mut Value, save_as: &str, value: Value) {
    ensure_workflow_steps_var(vars);
    if let Some(map) = vars.as_object_mut() {
        if let Some(Value::Object(steps)) = map.get_mut("steps") {
            steps.insert(save_as.to_string(), value);
        }
    }
}

fn render_workflow_output(
    template: &Value,
    vars: &Value,
    step_count: usize,
    trace: Vec<Value>,
) -> Value {
    let rendered = substitute_vars(template.clone(), vars);
    match rendered {
        Value::Object(mut obj) => {
            obj.insert("steps".to_string(), json!(step_count));
            obj.insert("trace".to_string(), json!(trace));
            Value::Object(obj)
        }
        other => json!({
            "output": other,
            "steps": step_count,
            "trace": trace
        }),
    }
}

fn substitute_exact_value(input: &str, vars: &Value) -> Option<Value> {
    let trimmed = input.trim();
    if !trimmed.starts_with("{{") || !trimmed.ends_with("}}") {
        return None;
    }
    let key = trimmed.trim_start_matches("{{").trim_end_matches("}}").trim();
    if key.is_empty() {
        return None;
    }
    lookup_var_value(vars, key)
}

fn substitute_string(input: &str, vars: &Value) -> String {
    let mut out = String::new();
    let mut rest = input;

    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start + 2..].find("}}") else {
            break;
        };
        let end = start + 2 + end;
        out.push_str(&rest[..start]);
        let key = rest[start + 2..end].trim();
        if let Some(repl) = lookup_var_string(vars, key) {
            out.push_str(&repl);
        } else {
            out.push_str(&rest[start..end + 2]);
        }
        rest = &rest[end + 2..];
    }

    out.push_str(rest);
    out
}

fn lookup_var_value(vars: &Value, key: &str) -> Option<Value> {
    let mut cur = vars;
    for part in key.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur.clone())
}

fn lookup_var_string(vars: &Value, key: &str) -> Option<String> {
    let value = lookup_var_value(vars, key)?;
    match value {
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

async fn driver_from_state(state: &AppState) -> Result<WebDriverClient> {
    let base_url = state
        .appium_base_url()
        .await
        .ok_or_else(|| {
            anyhow::Error::new(ToolCallError::new(
                ToolErrorCode::ActionFailed,
                "Appium is not initialized. Call ios.appium.ensure first.",
                json!({}),
            ))
        })?;
    let driver = WebDriverClient::new(&base_url).map_err(|err| {
        anyhow::Error::new(ToolCallError::new(
            ToolErrorCode::ActionFailed,
            format!("{err:#}"),
            json!({ "baseUrl": &base_url }),
        ))
    })?;
    Ok(driver)
}

async fn resolve_session_id(state: &AppState, arguments: &Value) -> Result<String> {
    if let Some(value) = arguments.get("sessionId").and_then(Value::as_str) {
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
    }

    state
        .active_session()
        .await
        .map(|session| session.session_id)
        .ok_or_else(|| {
            anyhow::Error::new(ToolCallError::new(
                ToolErrorCode::NoSession,
                "sessionId is required when no active session exists",
                json!({}),
            ))
        })
}

fn required_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow::Error::new(ToolCallError::new(
                ToolErrorCode::InvalidParams,
                format!("'{key}' is required"),
                json!({"param": key}),
            ))
        })
}

async fn wait_for_selector(
    driver: &WebDriverClient,
    session_id: &str,
    selector: &str,
    index: usize,
    require_unique: bool,
    timeout: Duration,
) -> Result<String> {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let ids = driver.find_elements_css(session_id, selector).await?;
        if ids.is_empty() {
            // keep waiting
        } else if require_unique && ids.len() != 1 {
            return Err(ToolCallError::new(
                ToolErrorCode::AmbiguousMatch,
                format!("expected exactly one match for selector '{selector}', got {}", ids.len()),
                json!({"selector": selector, "matchCount": ids.len()}),
            )
            .into());
        } else if let Some(value) = ids.get(index) {
            return Ok(value.clone());
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(ToolCallError::new(
                ToolErrorCode::Timeout,
                format!("timeout waiting for selector '{selector}'"),
                json!({"selector": selector, "index": index}),
            )
            .into());
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use serde_json::json;

    #[test]
    fn tool_error_from_anyhow_downcasts_tool_call_error() {
        let err = anyhow::Error::new(ToolCallError::new(
            ToolErrorCode::ElementNotFound,
            "missing",
            json!({"k": 1}),
        ));
        let value = tool_error_from_anyhow(&err, "ios.action.tap");
        let structured = value
            .get("structuredContent")
            .and_then(Value::as_object)
            .expect("structured");
        assert_eq!(
            structured.get("errorCode").and_then(Value::as_str),
            Some("ELEMENT_NOT_FOUND")
        );
        assert_eq!(
            structured
                .get("details")
                .and_then(|v| v.get("tool"))
                .and_then(Value::as_str),
            Some("ios.action.tap")
        );
    }

    #[tokio::test]
    async fn run_steps_blocks_requires_commit_with_error_code() {
        let state = AppState::new();
        let result = run_steps(
            &state,
            &[json!({"tool": "ios.web.goto", "requiresCommit": true, "arguments": {"url": "https://example.com"}})],
            false,
            &json!({}),
            None,
        )
        .await
        .expect("result");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            result.get("errorCode").and_then(Value::as_str),
            Some("COMMIT_REQUIRED")
        );
        let trace = result.get("trace").and_then(Value::as_array).expect("trace");
        assert_eq!(trace.len(), 1);
        assert_eq!(
            trace[0].get("errorCode").and_then(Value::as_str),
            Some("COMMIT_REQUIRED")
        );
    }

    #[tokio::test]
    async fn run_steps_preserves_tool_error_code() {
        let state = AppState::new();
        let result = run_steps(
            &state,
            &[json!({"tool": "ios.ui.source", "arguments": {}})],
            false,
            &json!({}),
            None,
        )
        .await
        .expect("result");

        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            result.get("errorCode").and_then(Value::as_str),
            Some("NO_SESSION")
        );
        let trace = result.get("trace").and_then(Value::as_array).expect("trace");
        assert!(!trace.is_empty());
        let last = trace.last().unwrap();
        assert_eq!(last.get("errorCode").and_then(Value::as_str), Some("NO_SESSION"));
    }
}
