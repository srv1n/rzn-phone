fn load_favorites() -> Result<Vec<String>> {
    let path = favorites_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let payload = serde_json::from_str::<Value>(&fs::read_to_string(path)?)?;
    let mut out = Vec::new();
    if let Some(values) = payload.as_array() {
        for value in values {
            if let Some(item) = value.as_str() {
                let normalized = canonicalize_workflow_ref(item);
                if !normalized.is_empty() && !out.contains(&normalized) {
                    out.push(normalized);
                }
            }
        }
    }
    Ok(out)
}

fn save_favorites(favorites: &[String]) -> Result<()> {
    let mut deduped = Vec::new();
    for value in favorites {
        let normalized = canonicalize_workflow_ref(value);
        if !normalized.is_empty() && !deduped.contains(&normalized) {
            deduped.push(normalized);
        }
    }
    write_private_file(
        &favorites_path()?,
        (serde_json::to_string_pretty(&deduped)? + "\n").as_bytes(),
    )?;
    Ok(())
}
