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

pub fn parse_xctrace_devices(input: &str, include_simulators: bool) -> Vec<DeviceInfo> {
    let line_regex = Regex::new(
        r"^(?P<name>.+?)\s+\((?P<version>[^)]+)\)\s+\((?P<udid>[A-Za-z0-9\-]+)\)(?:\s+\((?P<status>[^)]*)\))?$",
    )
    .expect("xctrace regex");

    let mut in_devices = false;
    let mut in_simulators = false;
    let mut out = Vec::new();

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if line == "== Devices ==" {
            in_devices = true;
            in_simulators = false;
            continue;
        }

        if line == "== Simulators ==" {
            in_devices = false;
            in_simulators = true;
            continue;
        }

        if !in_devices && !in_simulators {
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
            .unwrap_or(true);

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

#[cfg(test)]
mod tests {
    use super::parse_xctrace_devices;

    const DEVICES_ONLY: &str = r#"
== Devices ==
MacBook Pro (14.1)
Sara's iPhone (17.5.1) (00008110-001C12340E87801E)

== Simulators ==
"#;

    const WITH_SIMULATORS: &str = r#"
== Devices ==
iPhone 15 Pro (17.4) (00008110-001C12340E87801E)

== Simulators ==
-- iOS 17.4 --
iPhone 15 (17.4) (A1B2C3D4-1234-5678-9ABC-DEF012345678)
"#;

    const NOISY_LINES: &str = r#"
random heading
== Devices ==
Malformed line
John iPhone (17.3) (00008101-9999999999AAA)
Jane iPhone (17.2) (00008101-8888888888BBB) (unavailable, wirelessly disconnected)
== Simulators ==
"#;

    #[test]
    fn parses_physical_devices_only() {
        let parsed = parse_xctrace_devices(DEVICES_ONLY, false);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Sara's iPhone");
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
}
