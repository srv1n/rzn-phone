use anyhow::{Context, Result};
use regex::Regex;
use tokio::process::Command;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceInfo {
    pub udid: String,
    pub name: String,
    pub platform_version: String,
    pub model: String,
    pub is_simulator: bool,
    pub is_available: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceProbe {
    pub udid: String,
    pub name: Option<String>,
    pub platform_version: Option<String>,
    pub model: Option<String>,
    pub is_simulator: bool,
    pub is_available: bool,
    pub state: String,
    pub matched_section: Option<String>,
    pub matched_line: Option<String>,
}

pub async fn list_devices(include_simulators: bool) -> Result<Vec<DeviceInfo>> {
    let output = Command::new("xcrun")
        .args(["xctrace", "list", "devices"])
        .output()
        .await
        .context("failed to run xcrun xctrace list devices")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("xctrace failed: {}", stderr.trim());
    }

    let text = String::from_utf8(output.stdout).context("xctrace output is not UTF-8")?;
    Ok(parse_xctrace_devices(&text, include_simulators))
}

pub async fn probe_device(udid: &str) -> Result<DeviceProbe> {
    let output = Command::new("xcrun")
        .args(["xctrace", "list", "devices"])
        .output()
        .await
        .context("failed to run xcrun xctrace list devices")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("xctrace failed: {}", stderr.trim());
    }

    let text = String::from_utf8(output.stdout).context("xctrace output is not UTF-8")?;
    Ok(probe_xctrace_device(&text, udid))
}

pub fn parse_xctrace_devices(input: &str, include_simulators: bool) -> Vec<DeviceInfo> {
    let line_regex = Regex::new(
        r"^(?P<name>.+?)\s+\((?P<version>[^)]+)\)\s+\((?P<udid>[A-Za-z0-9\-]+)\)(?:\s+\((?P<status>[^)]*)\))?$",
    )
    .expect("xctrace regex");

    let mut in_devices = false;
    let mut in_offline_devices = false;
    let mut in_simulators = false;
    let mut out = Vec::new();

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if line == "== Devices ==" {
            in_devices = true;
            in_offline_devices = false;
            in_simulators = false;
            continue;
        }

        if line == "== Devices Offline ==" {
            in_devices = false;
            in_offline_devices = true;
            in_simulators = false;
            continue;
        }

        if line == "== Simulators ==" {
            in_devices = false;
            in_offline_devices = false;
            in_simulators = true;
            continue;
        }

        if !in_devices && !in_offline_devices && !in_simulators {
            continue;
        }

        if in_simulators && !include_simulators {
            continue;
        }

        if line.starts_with("--") && line.ends_with("--") {
            continue;
        }

        let Some(caps) = line_regex.captures(line) else {
            continue;
        };

        let name = caps
            .name("name")
            .map(|value| value.as_str().trim().to_string())
            .unwrap_or_default();
        let platform_version = caps
            .name("version")
            .map(|value| value.as_str().trim().to_string())
            .unwrap_or_default();
        let udid = caps
            .name("udid")
            .map(|value| value.as_str().trim().to_string())
            .unwrap_or_default();

        if name.is_empty() || udid.is_empty() {
            continue;
        }

        let status_text = caps
            .name("status")
            .map(|value| value.as_str().to_lowercase());
        let is_available = status_text
            .as_deref()
            .map(|text| !text.contains("unavailable"))
            .unwrap_or(!in_offline_devices)
            && !in_offline_devices;

        out.push(DeviceInfo {
            udid,
            name: name.clone(),
            platform_version,
            model: name,
            is_simulator: in_simulators,
            is_available,
        });
    }

    out
}

pub fn probe_xctrace_device(input: &str, target_udid: &str) -> DeviceProbe {
    let line_regex = Regex::new(
        r"^(?P<name>.+?)\s+\((?P<version>[^)]+)\)\s+\((?P<udid>[A-Za-z0-9\-]+)\)(?:\s+\((?P<status>[^)]*)\))?$",
    )
    .expect("xctrace regex");

    let mut section = "";

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if line == "== Devices ==" {
            section = "devices";
            continue;
        }

        if line == "== Devices Offline ==" {
            section = "devices_offline";
            continue;
        }

        if line == "== Simulators ==" {
            section = "simulators";
            continue;
        }

        if section.is_empty() || (line.starts_with("--") && line.ends_with("--")) {
            continue;
        }

        let Some(caps) = line_regex.captures(line) else {
            continue;
        };
        let Some(udid) = caps.name("udid").map(|value| value.as_str().trim()) else {
            continue;
        };
        if udid != target_udid {
            continue;
        }

        let name = caps
            .name("name")
            .map(|value| value.as_str().trim().to_string());
        let platform_version = caps
            .name("version")
            .map(|value| value.as_str().trim().to_string());
        let status_text = caps
            .name("status")
            .map(|value| value.as_str().to_lowercase())
            .unwrap_or_default();
        let is_simulator = section == "simulators";
        let is_available = section == "devices"
            && !status_text.contains("unavailable")
            && !status_text.contains("offline");
        let state = if is_simulator {
            "simulator"
        } else if section == "devices" && is_available {
            "available"
        } else if section == "devices_offline"
            || status_text.contains("unavailable")
            || status_text.contains("offline")
        {
            "offline"
        } else {
            "available"
        };

        return DeviceProbe {
            udid: target_udid.to_string(),
            name: name.clone(),
            platform_version,
            model: name,
            is_simulator,
            is_available,
            state: state.to_string(),
            matched_section: Some(section.to_string()),
            matched_line: Some(line.to_string()),
        };
    }

    DeviceProbe {
        udid: target_udid.to_string(),
        name: None,
        platform_version: None,
        model: None,
        is_simulator: false,
        is_available: false,
        state: "missing".to_string(),
        matched_section: None,
        matched_line: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_xctrace_devices, probe_xctrace_device};

    const DEVICES_ONLY: &str = r#"
== Devices ==
MacBook Pro (14.1)
Example iPhone (17.5.1) (TEST-UDID-IPHONE-001)

== Simulators ==
"#;

    const WITH_SIMULATORS: &str = r#"
== Devices ==
iPhone 15 Pro (17.4) (TEST-UDID-IPHONE-001)

== Simulators ==
-- iOS 17.4 --
iPhone 15 (17.4) (A1B2C3D4-1234-5678-9ABC-DEF012345678)
"#;

    const NOISY_LINES: &str = r#"
random heading
== Devices ==
Malformed line
Test iPhone A (17.3) (TEST-UDID-IPHONE-002)
Test iPhone B (17.2) (TEST-UDID-IPHONE-003) (unavailable, wirelessly disconnected)
== Simulators ==
"#;

    const WITH_OFFLINE_SECTION: &str = r#"
== Devices ==
Example iPad (18.7.7) (TEST-UDID-IPAD-001)

== Devices Offline ==
Offline Test Phone (18.6.2) (TEST-UDID-OFFLINE-001)

== Simulators ==
"#;

    #[test]
    fn parses_physical_devices_only() {
        let parsed = parse_xctrace_devices(DEVICES_ONLY, false);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Example iPhone");
        assert!(!parsed[0].is_simulator);
    }

    #[test]
    fn includes_simulators_when_enabled() {
        let parsed = parse_xctrace_devices(WITH_SIMULATORS, true);
        assert_eq!(parsed.len(), 2);
        assert!(parsed.iter().any(|device| device.is_simulator));
    }

    #[test]
    fn tolerates_noise_and_unavailable_state() {
        let parsed = parse_xctrace_devices(NOISY_LINES, false);
        assert_eq!(parsed.len(), 2);
        assert!(parsed.iter().any(|device| !device.is_available));
    }

    #[test]
    fn probe_device_marks_offline_devices() {
        let probe = probe_xctrace_device(WITH_OFFLINE_SECTION, "TEST-UDID-OFFLINE-001");
        assert_eq!(probe.state, "offline");
        assert_eq!(probe.name.as_deref(), Some("Offline Test Phone"));
        assert_eq!(probe.matched_section.as_deref(), Some("devices_offline"));
    }

    #[test]
    fn runtime_guardrails_parses_offline_devices_as_unavailable() {
        let parsed = parse_xctrace_devices(WITH_OFFLINE_SECTION, false);
        let offline = parsed
            .iter()
            .find(|device| device.udid == "TEST-UDID-OFFLINE-001")
            .expect("offline device");
        assert_eq!(offline.name, "Offline Test Phone");
        assert!(!offline.is_available);
        assert!(!offline.is_simulator);
    }

    #[test]
    fn probe_device_marks_missing_devices() {
        let probe = probe_xctrace_device(DEVICES_ONLY, "missing-udid");
        assert_eq!(probe.state, "missing");
        assert!(probe.matched_line.is_none());
    }
}
