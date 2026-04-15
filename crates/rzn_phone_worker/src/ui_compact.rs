use anyhow::{anyhow, Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde_json::Value;
use std::collections::HashMap;
use std::str;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SNAP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy)]
pub enum NodeFilter {
    Interactive,
    All,
}

impl NodeFilter {
    pub fn from_str(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "all" => Self::All,
            _ => Self::Interactive,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CompactNode {
    pub id: String,
    pub role: String,
    pub name: Option<String>,
    pub label: Option<String>,
    pub value: Option<String>,
    pub enabled: bool,
    pub visible: bool,
    pub bounds: Bounds,
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone)]
pub struct TargetLocator {
    pub using: String,
    pub value: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CompactStats {
    pub parsed_nodes: usize,
    pub included_nodes: usize,
    pub trimmed: bool,
}

pub struct CompactSnapshot {
    pub snapshot_id: String,
    pub nodes: Vec<CompactNode>,
    pub targets: HashMap<String, TargetLocator>,
    pub stats: CompactStats,
}

pub fn build_compact_snapshot(
    xml: &str,
    filter: NodeFilter,
    max_nodes: usize,
) -> Result<CompactSnapshot> {
    let max_nodes = max_nodes.clamp(10, 500);
    let snapshot_id = new_snapshot_id();

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = false;

    let mut buf = Vec::new();
    let mut parsed_nodes = 0usize;

    #[derive(Debug, Clone)]
    struct RawNode {
        role: String,
        name: Option<String>,
        label: Option<String>,
        value: Option<String>,
        enabled: bool,
        visible: bool,
        bounds: Bounds,
        hints: Vec<String>,
    }

    let mut raw: Vec<RawNode> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let elem_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if !elem_name.starts_with("XCUIElementType") {
                    buf.clear();
                    continue;
                }

                parsed_nodes += 1;

                let mut name: Option<String> = None;
                let mut label: Option<String> = None;
                let mut value: Option<String> = None;
                let mut enabled: bool = true;
                let mut visible: bool = true;
                let mut x: f64 = 0.0;
                let mut y: f64 = 0.0;
                let mut w: f64 = 0.0;
                let mut h: f64 = 0.0;

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
                        "enabled" => enabled = parse_bool(&val, true),
                        "visible" => visible = parse_bool(&val, true),
                        "x" => x = val.parse::<f64>().unwrap_or(0.0),
                        "y" => y = val.parse::<f64>().unwrap_or(0.0),
                        "width" => w = val.parse::<f64>().unwrap_or(0.0),
                        "height" => h = val.parse::<f64>().unwrap_or(0.0),
                        _ => {}
                    }
                }

                if !visible {
                    buf.clear();
                    continue;
                }

                let role = role_for_type(&elem_name);
                let interactive = is_interactive_role(&role);
                let include = match filter {
                    NodeFilter::Interactive => interactive,
                    NodeFilter::All => interactive || role == "text",
                };

                if !include {
                    buf.clear();
                    continue;
                }

                let mut hints: Vec<String> = Vec::new();
                if interactive && enabled {
                    hints.push("tap".to_string());
                    if role == "field" {
                        hints.push("type".to_string());
                    }
                }

                if interactive {
                    if name.as_deref().unwrap_or("").is_empty()
                        && label.as_deref().unwrap_or("").is_empty()
                    {
                        buf.clear();
                        continue;
                    }
                } else if role == "text" {
                    if label.as_deref().unwrap_or("").is_empty()
                        && value.as_deref().unwrap_or("").is_empty()
                    {
                        buf.clear();
                        continue;
                    }
                }

                raw.push(RawNode {
                    role,
                    name,
                    label,
                    value,
                    enabled,
                    visible,
                    bounds: Bounds { x, y, w, h },
                    hints,
                });
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(anyhow!("failed to parse XML: {err}")),
            _ => {}
        }
        buf.clear();
    }

    raw.sort_by(|a, b| {
        a.bounds
            .y
            .partial_cmp(&b.bounds.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.bounds
                    .x
                    .partial_cmp(&b.bounds.x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let trimmed = raw.len() > max_nodes;
    if trimmed {
        raw.truncate(max_nodes);
    }

    let mut counters: HashMap<&'static str, u64> = HashMap::new();
    let mut nodes: Vec<CompactNode> = Vec::with_capacity(raw.len());
    let mut targets: HashMap<String, TargetLocator> = HashMap::new();

    for node in raw {
        let prefix = prefix_for_role(&node.role);
        let counter = counters.entry(prefix).or_insert(0);
        *counter += 1;
        let encoded_id = format!("{prefix}_{}", *counter);

        if !node.hints.is_empty() {
            if let Some(locator) = build_locator(&node.name, &node.label) {
                targets.insert(encoded_id.clone(), locator);
            }
        }

        nodes.push(CompactNode {
            id: encoded_id,
            role: node.role,
            name: truncate(node.name, 140),
            label: truncate(node.label, 140),
            value: truncate(node.value, 140),
            enabled: node.enabled,
            visible: node.visible,
            bounds: node.bounds,
            hints: node.hints,
        });
    }

    let included_nodes = nodes.len();

    Ok(CompactSnapshot {
        snapshot_id,
        nodes,
        targets,
        stats: CompactStats {
            parsed_nodes,
            included_nodes,
            trimmed,
        },
    })
}

pub fn locator_to_json(locator: &TargetLocator) -> Value {
    serde_json::json!({
        "using": locator.using,
        "value": locator.value
    })
}

fn new_snapshot_id() -> String {
    let epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let seq = SNAP_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("snap_{epoch_ms}_{seq}")
}

fn normalize_text(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn truncate(value: Option<String>, max_len: usize) -> Option<String> {
    let Some(value) = value else { return None };
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if idx >= max_len {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    Some(out)
}

fn parse_bool(value: &str, default: bool) -> bool {
    match value.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" => true,
        "false" | "0" | "no" => false,
        _ => default,
    }
}

fn role_for_type(type_name: &str) -> String {
    if type_name.ends_with("Button") {
        "button".to_string()
    } else if type_name.ends_with("TextField") || type_name.ends_with("SecureTextField") {
        "field".to_string()
    } else if type_name.ends_with("TextView") {
        "field".to_string()
    } else if type_name.ends_with("Cell") {
        "cell".to_string()
    } else if type_name.ends_with("Link") {
        "link".to_string()
    } else if type_name.ends_with("Switch") {
        "switch".to_string()
    } else if type_name.ends_with("StaticText") {
        "text".to_string()
    } else {
        "other".to_string()
    }
}

fn is_interactive_role(role: &str) -> bool {
    matches!(role, "button" | "field" | "cell" | "link" | "switch")
}

fn prefix_for_role(role: &str) -> &'static str {
    match role {
        "button" => "btn",
        "field" => "fld",
        "cell" => "cell",
        "link" => "lnk",
        "switch" => "sw",
        "text" => "txt",
        _ => "el",
    }
}

fn build_locator(name: &Option<String>, label: &Option<String>) -> Option<TargetLocator> {
    if let Some(name) = name.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        return Some(TargetLocator {
            using: "accessibility id".to_string(),
            value: name.to_string(),
        });
    }

    if let Some(label) = label.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        return Some(TargetLocator {
            using: "ios predicate string".to_string(),
            value: format!("label == \"{}\"", escape_predicate_value(label)),
        });
    }

    None
}

fn escape_predicate_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::{build_compact_snapshot, NodeFilter};

    #[test]
    fn builds_compact_snapshot_with_encoded_ids() {
        let xml = r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <AppiumAUT>
          <XCUIElementTypeApplication>
            <XCUIElementTypeButton name="ok" label="OK" enabled="true" visible="true" x="10" y="20" width="30" height="40"/>
            <XCUIElementTypeStaticText label="Hello" visible="true" x="10" y="80" width="100" height="20"/>
            <XCUIElementTypeTextField name="email" label="Email" enabled="true" visible="true" x="10" y="120" width="200" height="40"/>
          </XCUIElementTypeApplication>
        </AppiumAUT>
        "#;

        let snapshot = build_compact_snapshot(xml, NodeFilter::All, 50).expect("snapshot");
        assert!(!snapshot.nodes.is_empty());
        assert!(snapshot.nodes.iter().any(|n| n.id.starts_with("btn_")));
        assert!(snapshot.nodes.iter().any(|n| n.id.starts_with("fld_")));
        assert!(snapshot.nodes.iter().any(|n| n.id.starts_with("txt_")));
        assert!(!snapshot.targets.is_empty());
    }

    #[test]
    fn interactive_filter_excludes_static_text() {
        let xml = r#"
        <AppiumAUT>
          <XCUIElementTypeApplication>
            <XCUIElementTypeStaticText label="Hello" visible="true"/>
            <XCUIElementTypeButton name="ok" label="OK" visible="true"/>
          </XCUIElementTypeApplication>
        </AppiumAUT>
        "#;

        let snapshot = build_compact_snapshot(xml, NodeFilter::Interactive, 50).expect("snapshot");
        assert_eq!(snapshot.nodes.len(), 1);
        assert_eq!(snapshot.nodes[0].role, "button");
    }
}
