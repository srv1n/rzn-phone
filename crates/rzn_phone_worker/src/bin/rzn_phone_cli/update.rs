async fn update_workflows(
    runtime: &RuntimePaths,
    source: Option<String>,
    version: Option<String>,
) -> Result<()> {
    let source = source
        .or_else(|| default_update_source(runtime))
        .ok_or_else(|| anyhow!("no workflow update source configured; pass --source"))?;
    let version = if let Some(version) = version {
        version
    } else {
        discover_workflow_pack_version(&source).await?
    };
    let archive_ref = resolve_archive_ref(&source, &version);
    let tmp_root = env::temp_dir().join(format!("rzn-phone-workflows-{}", std::process::id()));
    if tmp_root.exists() {
        fs::remove_dir_all(&tmp_root)?;
    }
    fs::create_dir_all(&tmp_root)?;
    let archive_path = tmp_root.join("workflows.tar.gz");
    stage_workflow_archive(&archive_ref, &archive_path).await?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&tmp_root)
        .status()?;
    if !status.success() {
        bail!("failed to extract workflow archive");
    }
    let pack_root = tmp_root.join("rzn-phone-workflows");
    if !pack_root.join("resources/workflows").is_dir() {
        bail!("workflow pack is missing resources/workflows");
    }
    if !pack_root.join("examples").is_dir() {
        bail!("workflow pack is missing examples");
    }
    if runtime.workflow_dir.exists() {
        fs::remove_dir_all(&runtime.workflow_dir)?;
    }
    if runtime.systems_dir.exists() {
        fs::remove_dir_all(&runtime.systems_dir)?;
    }
    if runtime.examples_dir.exists() {
        fs::remove_dir_all(&runtime.examples_dir)?;
    }
    if runtime.skills_dir.exists() && pack_root.join("skills").is_dir() {
        fs::remove_dir_all(&runtime.skills_dir)?;
    }
    copy_dir_all(
        &pack_root.join("resources/workflows"),
        &runtime.workflow_dir,
    )?;
    copy_dir_all(&pack_root.join("resources/systems"), &runtime.systems_dir)?;
    copy_dir_all(&pack_root.join("examples"), &runtime.examples_dir)?;
    if pack_root.join("skills").is_dir() {
        copy_dir_all(&pack_root.join("skills"), &runtime.skills_dir)?;
    }
    if pack_root.join("VERSION").is_file() {
        fs::copy(
            pack_root.join("VERSION"),
            &runtime.workflow_pack_version_file,
        )?;
    }
    fs::write(&runtime.update_source_file, format!("{}\n", source))?;
    let workflow_count = fs::read_dir(&runtime.workflow_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
        .count();
    println!("Updated workflows from {}", source);
    println!("Workflow pack version: {}", workflow_pack_version(runtime)?);
    println!("Installed workflows: {}", workflow_count);
    let _ = fs::remove_dir_all(tmp_root);
    Ok(())
}

async fn discover_workflow_pack_version(source: &str) -> Result<String> {
    let value = if source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("file://")
    {
        read_source_text(&format!("{}/VERSION", source.trim_end_matches('/'))).await?
    } else if Path::new(source).is_dir() {
        fs::read_to_string(Path::new(source).join("VERSION"))?
    } else {
        String::new()
    };
    let version = value.trim().to_string();
    if version.is_empty() {
        bail!("unable to determine workflow pack version from source; pass --version");
    }
    Ok(version)
}

fn resolve_archive_ref(source: &str, version: &str) -> String {
    let archive_name = format!("rzn-phone-workflows-{}.tar.gz", version);
    if source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("file://")
    {
        if source.ends_with(".tar.gz") {
            source.to_string()
        } else {
            format!("{}/{}", source.trim_end_matches('/'), archive_name)
        }
    } else if Path::new(source).is_dir() {
        Path::new(source).join(archive_name).display().to_string()
    } else {
        source.to_string()
    }
}

async fn read_source_text(source: &str) -> Result<String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        Ok(reqwest::get(source)
            .await?
            .error_for_status()?
            .text()
            .await?)
    } else if let Some(path) = source.strip_prefix("file://") {
        Ok(fs::read_to_string(path)?)
    } else {
        Ok(fs::read_to_string(source)?)
    }
}

async fn stage_workflow_archive(source: &str, target: &Path) -> Result<()> {
    if source.starts_with("http://") || source.starts_with("https://") {
        let bytes = reqwest::get(source)
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        fs::write(target, bytes)?;
    } else if let Some(path) = source.strip_prefix("file://") {
        fs::copy(path, target)?;
    } else {
        fs::copy(source, target)?;
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dest.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
