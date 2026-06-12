fn install_skill_links(
    runtime: &RuntimePaths,
    args: &SkillInstallArgs,
    update: bool,
) -> Result<Value> {
    let skill = validate_skill_name(&args.skill)?;
    let source = bundled_skill_source(runtime, &skill)?;
    let version = skill_version(runtime)?;
    let clients = resolve_skill_clients(&args.clients)?;
    let project_dir = resolve_project_dir(args.project_dir.as_ref())?;
    let mut results = Vec::new();

    for client in clients {
        let base_dir = client_skill_base_dir(client, args.scope, &project_dir)?;
        let target = base_dir.join(&skill);
        let action = install_skill_symlink(&source, &target, args.force)?;
        write_skill_manifest(
            &skill_manifest_path(&base_dir, &skill),
            &json!({
                "installer": "rzn-phone",
                "skill": skill,
                "client": client,
                "scope": skill_scope_label(args.scope),
                "source": source,
                "target": target,
                "version": version,
            }),
        )?;
        results.push(json!({
            "client": client,
            "clientLabel": skill_client_label(client),
            "scope": skill_scope_label(args.scope),
            "action": if update && action != "linked" { "updated" } else { action },
            "path": target,
            "source": source,
            "version": version,
        }));
    }

    Ok(json!({
        "skill": skill,
        "scope": skill_scope_label(args.scope),
        "projectDir": project_dir,
        "source": source,
        "version": version,
        "results": results,
    }))
}

fn remove_skill_links(args: &SkillRemoveArgs) -> Result<Value> {
    let skill = validate_skill_name(&args.skill)?;
    let clients = resolve_skill_clients(&args.clients)?;
    let project_dir = resolve_project_dir(args.project_dir.as_ref())?;
    let mut results = Vec::new();

    for client in clients {
        let base_dir = client_skill_base_dir(client, args.scope, &project_dir)?;
        let target = base_dir.join(&skill);
        let manifest = skill_manifest_path(&base_dir, &skill);
        let action = remove_skill_symlink(&target)?;
        if manifest.exists() {
            fs::remove_file(&manifest)?;
        }
        results.push(json!({
            "client": client,
            "clientLabel": skill_client_label(client),
            "scope": skill_scope_label(args.scope),
            "action": action,
            "path": target,
        }));
    }

    Ok(json!({
        "skill": skill,
        "scope": skill_scope_label(args.scope),
        "projectDir": project_dir,
        "results": results,
    }))
}

fn skill_status_payload(runtime: &RuntimePaths, args: &SkillStatusArgs) -> Result<Value> {
    let skill = validate_skill_name(&args.skill)?;
    let source = bundled_skill_source(runtime, &skill).ok();
    let version = skill_version(runtime).ok();
    let clients = resolve_skill_clients(&args.clients)?;
    let project_dir = resolve_project_dir(args.project_dir.as_ref())?;
    let mut results = Vec::new();

    for client in clients {
        let base_dir = client_skill_base_dir(client, args.scope, &project_dir)?;
        let target = base_dir.join(&skill);
        let manifest_path = skill_manifest_path(&base_dir, &skill);
        let manifest = read_skill_manifest(&manifest_path);
        results.push(inspect_skill_target(
            client,
            args.scope,
            &target,
            source.as_deref(),
            manifest.as_ref(),
        ));
    }

    Ok(json!({
        "skill": skill,
        "scope": skill_scope_label(args.scope),
        "projectDir": project_dir,
        "source": source,
        "version": version,
        "sourceExists": source.as_ref().map(|path| path.join("SKILL.md").is_file()).unwrap_or(false),
        "results": results,
    }))
}

fn bundled_skills_payload(runtime: &RuntimePaths) -> Result<Value> {
    let mut skills = Vec::new();
    if runtime.skills_dir.is_dir() {
        for entry in fs::read_dir(&runtime.skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.join("SKILL.md").is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            skills.push(json!({
                "name": name,
                "path": path,
            }));
        }
    }
    skills.sort_by_key(|item| {
        item.get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    });
    Ok(json!({
        "skillsDir": runtime.skills_dir,
        "version": skill_version(runtime).ok(),
        "count": skills.len(),
        "skills": skills,
    }))
}

fn validate_skill_name(raw: &str) -> Result<String> {
    let value = raw.trim();
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
    {
        bail!("skill name must be a simple folder name");
    }
    Ok(value.to_string())
}

fn bundled_skill_source(runtime: &RuntimePaths, skill: &str) -> Result<PathBuf> {
    let source = runtime.skills_dir.join(skill);
    if !source.join("SKILL.md").is_file() {
        bail!(
            "bundled skill '{}' was not found under {}",
            skill,
            runtime.skills_dir.display()
        );
    }
    Ok(source.canonicalize().unwrap_or(source))
}

fn skill_version(runtime: &RuntimePaths) -> Result<String> {
    workflow_pack_version(runtime).or_else(|_| runtime_version(runtime))
}

fn resolve_skill_clients(raw: &str) -> Result<Vec<&'static str>> {
    let mut out = Vec::new();
    for item in raw.split(',') {
        let key = item.trim().to_ascii_lowercase();
        let values: Vec<&'static str> = match key.as_str() {
            "" => Vec::new(),
            "all" | "*" => vec!["claude", "gemini", "agent", "codex"],
            "claude" | "claude-code" | "claude_code" => vec!["claude"],
            "gemini" => vec!["gemini"],
            "agent" | "agents" => vec!["agent"],
            "codex" => vec!["codex"],
            other => bail!("unknown skill client '{}'", other),
        };
        for value in values {
            if !out.contains(&value) {
                out.push(value);
            }
        }
    }
    if out.is_empty() {
        bail!("at least one skill client is required");
    }
    Ok(out)
}

fn skill_client_label(client: &str) -> &'static str {
    match client {
        "claude" => "Claude Code",
        "gemini" => "Gemini",
        "agent" => "Agent",
        "codex" => "Codex",
        _ => "Unknown",
    }
}

fn skill_scope_label(scope: SkillScope) -> &'static str {
    match scope {
        SkillScope::Global => "global",
        SkillScope::Project => "project",
    }
}

fn resolve_project_dir(project_dir: Option<&PathBuf>) -> Result<PathBuf> {
    let dir = project_dir
        .cloned()
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    Ok(dir.canonicalize().unwrap_or(dir))
}

fn client_skill_base_dir(client: &str, scope: SkillScope, project_dir: &Path) -> Result<PathBuf> {
    if scope == SkillScope::Project {
        let folder = match client {
            "claude" => ".claude/skills",
            "gemini" => ".gemini/skills",
            "agent" => ".agents/skills",
            "codex" => ".codex/skills",
            _ => bail!("unknown skill client '{}'", client),
        };
        return Ok(project_dir.join(folder));
    }

    let home = home_dir()?;
    let dir = match client {
        "claude" => env::var_os("CLAUDE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"))
            .join("skills"),
        "gemini" => env::var_os("GEMINI_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".gemini"))
            .join("skills"),
        "agent" => env::var_os("AGENTS_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".agents"))
            .join("skills"),
        "codex" => env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"))
            .join("skills"),
        _ => bail!("unknown skill client '{}'", client),
    };
    Ok(dir)
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("unable to determine home directory"))
}

fn skill_manifest_path(base_dir: &Path, skill: &str) -> PathBuf {
    base_dir.join(format!(".rzn-phone-skill-{}.json", skill))
}

fn install_skill_symlink(source: &Path, target: &Path, force: bool) -> Result<&'static str> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if !metadata.file_type().is_symlink() {
                bail!(
                    "{} already exists and is not a symlink; refusing to overwrite it",
                    target.display()
                );
            }
            let current = fs::read_link(target)?;
            let current_abs = if current.is_absolute() {
                current
            } else {
                target
                    .parent()
                    .map(|parent| parent.join(&current))
                    .unwrap_or(current)
            };
            if paths_equivalent(&current_abs, source) && !force {
                return Ok("unchanged");
            }
            remove_symlink_path(target, &metadata)?;
            create_dir_symlink(source, target)?;
            Ok("updated")
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            create_dir_symlink(source, target)?;
            Ok("linked")
        }
        Err(err) => Err(err).with_context(|| format!("unable to inspect {}", target.display())),
    }
}

fn remove_skill_symlink(target: &Path) -> Result<&'static str> {
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                remove_symlink_path(target, &metadata)?;
                Ok("removed")
            } else {
                Ok("skipped-not-symlink")
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok("missing"),
        Err(err) => Err(err).with_context(|| format!("unable to inspect {}", target.display())),
    }
}

fn remove_symlink_path(target: &Path, metadata: &fs::Metadata) -> Result<()> {
    if metadata.is_dir() && cfg!(windows) {
        fs::remove_dir(target)?;
    } else {
        fs::remove_file(target)?;
    }
    Ok(())
}

#[cfg(unix)]
fn create_dir_symlink(source: &Path, target: &Path) -> Result<()> {
    std::os::unix::fs::symlink(source, target)?;
    Ok(())
}

#[cfg(windows)]
fn create_dir_symlink(source: &Path, target: &Path) -> Result<()> {
    std::os::windows::fs::symlink_dir(source, target)?;
    Ok(())
}

fn paths_equivalent(a: &Path, b: &Path) -> bool {
    let left = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
    let right = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    left == right
}

fn write_skill_manifest(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

fn read_skill_manifest(path: &Path) -> Option<Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

fn inspect_skill_target(
    client: &str,
    scope: SkillScope,
    target: &Path,
    expected_source: Option<&Path>,
    manifest: Option<&Value>,
) -> Value {
    let mut status = "missing".to_string();
    let mut source = None;
    let mut source_exists = false;

    if let Ok(metadata) = fs::symlink_metadata(target) {
        if metadata.file_type().is_symlink() {
            match fs::read_link(target) {
                Ok(link) => {
                    let abs_link = if link.is_absolute() {
                        link
                    } else {
                        target
                            .parent()
                            .map(|parent| parent.join(&link))
                            .unwrap_or(link)
                    };
                    source_exists = abs_link.join("SKILL.md").is_file();
                    status = if expected_source
                        .map(|expected| paths_equivalent(&abs_link, expected))
                        .unwrap_or(false)
                    {
                        "installed".to_string()
                    } else {
                        "stale".to_string()
                    };
                    source = Some(abs_link);
                }
                Err(_) => {
                    status = "broken".to_string();
                }
            }
        } else {
            status = "conflict".to_string();
        }
    }

    json!({
        "client": client,
        "clientLabel": skill_client_label(client),
        "scope": skill_scope_label(scope),
        "status": status,
        "path": target,
        "source": source,
        "sourceExists": source_exists,
        "version": manifest
            .and_then(|value| value.get("version"))
            .and_then(Value::as_str),
    })
}
