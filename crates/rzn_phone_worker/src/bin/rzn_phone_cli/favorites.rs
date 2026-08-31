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
                if let Ok(reference) = parse_workflow_ref(item) {
                    if !out.contains(&reference) {
                        out.push(reference);
                    }
                }
            }
        }
    }
    Ok(out)
}

fn save_favorites(favorites: &[String]) -> Result<()> {
    let mut deduped = Vec::new();
    for value in favorites {
        if let Ok(reference) = parse_workflow_ref(value) {
            if !deduped.contains(&reference) {
                deduped.push(reference);
            }
        }
    }
    write_private_file(
        &favorites_path()?,
        (serde_json::to_string_pretty(&deduped)? + "\n").as_bytes(),
    )?;
    Ok(())
}
