use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::io::Read;
use tempfile::{Builder as TempDirBuilder, TempDir};

const WORKFLOW_ARCHIVE_ROOT: &str = "rzn-phone-workflows";
const RELEASE_PUBLIC_KEY_B64: &str =
    include_str!("../../../../../scripts/rzn_phone_release_ed25519.pub");
const SAFE_MODE_MASK: u32 = 0o7777;
const SPECIAL_MODE_BITS: u32 = 0o7000;

async fn update_workflows(
    runtime: &RuntimePaths,
    source: Option<String>,
    version: Option<String>,
) -> Result<()> {
    let source = source
        .or_else(|| default_update_source(runtime))
        .ok_or_else(|| anyhow!("no workflow update source configured; pass --source"))?;
    reject_plaintext_http_source(&source)?;
    let version = if let Some(version) = version {
        version
    } else {
        discover_workflow_pack_version(&source).await?
    };
    let archive_ref = resolve_archive_ref(&source, &version);
    reject_plaintext_http_source(&archive_ref)?;
    let tmp_dir = create_private_tempdir()?;
    let archive_name = archive_file_name(&archive_ref)?;
    let archive_path = tmp_dir.path().join(&archive_name);
    let sha_path = tmp_dir.path().join(format!("{}.sha256", archive_name));
    let sig_path = tmp_dir.path().join(format!("{}.sig", archive_name));
    stage_source_to_file(&archive_ref, &archive_path).await?;
    stage_source_to_file(&resolve_sha_ref(&archive_ref), &sha_path).await?;
    if is_remote_source(&archive_ref) {
        stage_source_to_file(&resolve_sig_ref(&archive_ref), &sig_path).await?;
        verify_release_signature(&archive_path, &sig_path)?;
    }
    verify_sha256(&archive_path, &sha_path)?;
    safe_extract_archive(&archive_path, tmp_dir.path(), WORKFLOW_ARCHIVE_ROOT)?;
    let pack_root = tmp_dir.path().join(WORKFLOW_ARCHIVE_ROOT);
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
    Ok(())
}

async fn discover_workflow_pack_version(source: &str) -> Result<String> {
    reject_plaintext_http_source(source)?;
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
    let archive_name = format!("{}-{}.tar.gz", WORKFLOW_ARCHIVE_ROOT, version);
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

fn resolve_sha_ref(archive_ref: &str) -> String {
    format!("{}.sha256", archive_ref)
}

fn resolve_sig_ref(archive_ref: &str) -> String {
    format!("{}.sig", archive_ref)
}

fn is_remote_source(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

async fn read_source_text(source: &str) -> Result<String> {
    reject_plaintext_http_source(source)?;
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

async fn stage_source_to_file(source: &str, target: &Path) -> Result<()> {
    reject_plaintext_http_source(source)?;
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

fn reject_plaintext_http_source(source: &str) -> Result<()> {
    if source.starts_with("http://") {
        bail!("plaintext HTTP workflow update sources are not allowed; use HTTPS or a local path");
    }
    Ok(())
}

fn bundled_release_public_key() -> Result<VerifyingKey> {
    let key_bytes = decode_base64_fixed::<32>(RELEASE_PUBLIC_KEY_B64, "release public key")?;
    VerifyingKey::from_bytes(&key_bytes).context("invalid bundled release public key")
}

fn verify_release_signature(archive_path: &Path, sig_path: &Path) -> Result<()> {
    verify_release_signature_with_key(archive_path, sig_path, &bundled_release_public_key()?)
}

fn verify_release_signature_with_key(
    archive_path: &Path,
    sig_path: &Path,
    public_key: &VerifyingKey,
) -> Result<()> {
    let signature_bytes = decode_base64_fixed::<64>(
        &fs::read_to_string(sig_path)
            .with_context(|| format!("failed to read signature {}", sig_path.display()))?,
        "release signature",
    )?;
    let signature = Signature::from_slice(&signature_bytes).context("invalid release signature")?;
    let archive = fs::read(archive_path)
        .with_context(|| format!("failed to read archive {}", archive_path.display()))?;
    public_key
        .verify(&archive, &signature)
        .with_context(|| format!("signature verification failed for {}", archive_path.display()))
}

fn decode_base64_fixed<const N: usize>(encoded: &str, label: &str) -> Result<[u8; N]> {
    let compact: String = encoded.split_whitespace().collect();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(compact.as_bytes())
        .with_context(|| format!("invalid base64 {}", label))?;
    decoded.try_into().map_err(|decoded: Vec<u8>| {
        anyhow!(
            "invalid {} length: expected {} bytes, got {}",
            label,
            N,
            decoded.len()
        )
    })
}

fn archive_file_name(archive_ref: &str) -> Result<String> {
    let path_without_query = archive_ref.split('?').next().unwrap_or(archive_ref);
    let name = Path::new(path_without_query)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("workflow archive must be a .tar.gz file: {}", archive_ref))?;
    if !name.ends_with(".tar.gz") {
        bail!("workflow archive must be a .tar.gz file: {}", archive_ref);
    }
    Ok(name.to_string())
}

fn create_private_tempdir() -> Result<TempDir> {
    let tempdir = TempDirBuilder::new()
        .prefix("rzn-phone-workflows.")
        .tempdir()?;
    #[cfg(unix)]
    {
        fs::set_permissions(tempdir.path(), fs::Permissions::from_mode(0o700))?;
    }
    Ok(tempdir)
}

fn verify_sha256(archive_path: &Path, sha_path: &Path) -> Result<()> {
    let archive_name = archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            anyhow!(
                "workflow archive path has no file name: {}",
                archive_path.display()
            )
        })?;
    let expected = expected_sha256(sha_path, archive_name)?;
    let actual = sha256_file(archive_path)?;
    if actual != expected {
        bail!(
            "sha256 mismatch for {}: expected {}, got {}",
            archive_name,
            expected,
            actual
        );
    }
    Ok(())
}

fn expected_sha256(sha_path: &Path, archive_name: &str) -> Result<String> {
    let mut fallback = None;
    for line in fs::read_to_string(sha_path)?.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.is_empty() {
            continue;
        }
        let digest = fields[0];
        if !is_sha256_hex(digest) {
            continue;
        }
        if fields.len() == 1 {
            fallback = Some(digest.to_ascii_lowercase());
            continue;
        }
        if fields[1..]
            .iter()
            .map(|name| name.trim_start_matches('*'))
            .any(|name| name == archive_name)
        {
            return Ok(digest.to_ascii_lowercase());
        }
    }
    fallback.ok_or_else(|| {
        anyhow!(
            "no sha256 entry found for {} in {}",
            archive_name,
            sha_path.display()
        )
    })
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn safe_extract_archive(archive_path: &Path, dest_dir: &Path, root_name: &str) -> Result<()> {
    validate_archive_members(archive_path, root_name)?;
    let file = fs::File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(dest_dir).with_context(|| {
        format!(
            "failed to extract workflow archive {}",
            archive_path.display()
        )
    })?;
    Ok(())
}

fn validate_archive_members(archive_path: &Path, root_name: &str) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut member_count = 0usize;
    for entry in archive
        .entries()
        .with_context(|| format!("invalid tar archive {}", archive_path.display()))?
    {
        let entry = entry?;
        validate_archive_member(&entry, root_name)?;
        member_count += 1;
    }
    if member_count == 0 {
        bail!("archive is empty");
    }
    Ok(())
}

fn validate_archive_member<R: Read>(entry: &tar::Entry<'_, R>, root_name: &str) -> Result<()> {
    let path = entry.path()?.into_owned();
    let name = path
        .to_str()
        .ok_or_else(|| anyhow!("archive member path is not valid UTF-8"))?;
    validate_archive_member_path(name, root_name)?;

    let entry_type = entry.header().entry_type();
    if entry_type.is_symlink() || entry_type.is_hard_link() {
        bail!("archive links are not allowed: {:?}", name);
    }
    if !(entry_type.is_file() || entry_type.is_dir()) {
        bail!("archive member type is not allowed: {:?}", name);
    }

    let mode = entry.header().mode()? & SAFE_MODE_MASK;
    if mode & SPECIAL_MODE_BITS != 0 {
        bail!("archive member has special mode bits: {:?}", name);
    }
    if mode & 0o002 != 0 {
        bail!("archive member is world-writable: {:?}", name);
    }
    Ok(())
}

fn validate_archive_member_path(name: &str, root_name: &str) -> Result<()> {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') {
        bail!("unsafe archive path: {:?}", name);
    }
    let trimmed = name.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        bail!("unsafe archive path: {:?}", name);
    }
    let parts: Vec<_> = trimmed.split('/').collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || *part == "." || *part == "..")
    {
        bail!("unsafe archive path: {:?}", name);
    }
    if parts.first().copied() != Some(root_name) {
        bail!("archive member is outside {}: {:?}", root_name, name);
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

#[cfg(test)]
mod update_tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io;

    #[derive(Clone)]
    enum TestMemberKind {
        File(Vec<u8>),
        Dir,
        Symlink(String),
        Hardlink(String),
        Fifo,
    }

    #[derive(Clone)]
    struct TestMember {
        name: String,
        mode: u32,
        kind: TestMemberKind,
    }

    fn file_member(name: &str, mode: u32, payload: &[u8]) -> TestMember {
        TestMember {
            name: name.to_string(),
            mode,
            kind: TestMemberKind::File(payload.to_vec()),
        }
    }

    fn dir_member(name: &str, mode: u32) -> TestMember {
        TestMember {
            name: name.to_string(),
            mode,
            kind: TestMemberKind::Dir,
        }
    }

    fn symlink_member(name: &str, target: &str) -> TestMember {
        TestMember {
            name: name.to_string(),
            mode: 0o755,
            kind: TestMemberKind::Symlink(target.to_string()),
        }
    }

    fn hardlink_member(name: &str, target: &str) -> TestMember {
        TestMember {
            name: name.to_string(),
            mode: 0o644,
            kind: TestMemberKind::Hardlink(target.to_string()),
        }
    }

    fn fifo_member(name: &str) -> TestMember {
        TestMember {
            name: name.to_string(),
            mode: 0o644,
            kind: TestMemberKind::Fifo,
        }
    }

    fn valid_pack_members(version: &str) -> Vec<TestMember> {
        vec![
            dir_member(WORKFLOW_ARCHIVE_ROOT, 0o755),
            dir_member(&format!("{}/resources", WORKFLOW_ARCHIVE_ROOT), 0o755),
            dir_member(
                &format!("{}/resources/workflows", WORKFLOW_ARCHIVE_ROOT),
                0o755,
            ),
            file_member(
                &format!("{}/resources/workflows/new.json", WORKFLOW_ARCHIVE_ROOT),
                0o644,
                br#"{"id":"new"}"#,
            ),
            dir_member(
                &format!("{}/resources/systems", WORKFLOW_ARCHIVE_ROOT),
                0o755,
            ),
            file_member(
                &format!("{}/resources/systems/default.json", WORKFLOW_ARCHIVE_ROOT),
                0o644,
                br#"{"id":"default"}"#,
            ),
            dir_member(&format!("{}/examples", WORKFLOW_ARCHIVE_ROOT), 0o755),
            file_member(
                &format!("{}/examples/example.json", WORKFLOW_ARCHIVE_ROOT),
                0o644,
                br#"{"example":true}"#,
            ),
            file_member(
                &format!("{}/VERSION", WORKFLOW_ARCHIVE_ROOT),
                0o644,
                format!("{}\n", version).as_bytes(),
            ),
        ]
    }

    fn write_archive(path: &Path, members: &[TestMember]) {
        let file = fs::File::create(path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for member in members {
            let mut header = tar::Header::new_gnu();
            set_raw_header_path(&mut header, &member.name);
            header.set_mode(member.mode);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mtime(0);
            match &member.kind {
                TestMemberKind::File(payload) => {
                    header.set_entry_type(tar::EntryType::Regular);
                    header.set_size(payload.len() as u64);
                    header.set_cksum();
                    builder.append(&header, payload.as_slice()).unwrap();
                }
                TestMemberKind::Dir => {
                    header.set_entry_type(tar::EntryType::Directory);
                    header.set_size(0);
                    header.set_cksum();
                    builder.append(&header, io::empty()).unwrap();
                }
                TestMemberKind::Symlink(target) => {
                    header.set_entry_type(tar::EntryType::Symlink);
                    header.set_size(0);
                    header.set_link_name(target).unwrap();
                    header.set_cksum();
                    builder.append(&header, io::empty()).unwrap();
                }
                TestMemberKind::Hardlink(target) => {
                    header.set_entry_type(tar::EntryType::Link);
                    header.set_size(0);
                    header.set_link_name(target).unwrap();
                    header.set_cksum();
                    builder.append(&header, io::empty()).unwrap();
                }
                TestMemberKind::Fifo => {
                    header.set_entry_type(tar::EntryType::Fifo);
                    header.set_size(0);
                    header.set_cksum();
                    builder.append(&header, io::empty()).unwrap();
                }
            }
        }
        builder.finish().unwrap();
    }

    fn set_raw_header_path(header: &mut tar::Header, name: &str) {
        let bytes = name.as_bytes();
        assert!(bytes.len() < header.as_old().name.len());
        let raw_name = &mut header.as_old_mut().name;
        raw_name.fill(0);
        raw_name[..bytes.len()].copy_from_slice(bytes);
    }

    fn write_source_archive(source: &Path, version: &str, members: &[TestMember]) -> PathBuf {
        fs::create_dir_all(source).unwrap();
        fs::write(source.join("VERSION"), format!("{}\n", version)).unwrap();
        let archive_path = source.join(format!("{}-{}.tar.gz", WORKFLOW_ARCHIVE_ROOT, version));
        write_archive(&archive_path, members);
        write_matching_sha256(&archive_path);
        archive_path
    }

    fn write_matching_sha256(archive_path: &Path) {
        let digest = sha256_file(archive_path).unwrap();
        let archive_name = archive_path.file_name().unwrap().to_str().unwrap();
        fs::write(
            archive_path.with_file_name(format!("{}.sha256", archive_name)),
            format!("{}  {}\n", digest, archive_name),
        )
        .unwrap();
    }

    fn test_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn write_matching_signature(archive_path: &Path, signing_key: &SigningKey) {
        let archive = fs::read(archive_path).unwrap();
        let signature = signing_key.sign(&archive);
        let archive_name = archive_path.file_name().unwrap().to_str().unwrap();
        fs::write(
            archive_path.with_file_name(format!("{}.sig", archive_name)),
            format!(
                "{}\n",
                base64::engine::general_purpose::STANDARD.encode(signature.to_bytes())
            ),
        )
        .unwrap();
    }

    fn write_mismatched_sha256(archive_path: &Path) {
        let archive_name = archive_path.file_name().unwrap().to_str().unwrap();
        fs::write(
            archive_path.with_file_name(format!("{}.sha256", archive_name)),
            format!("{}  {}\n", "0".repeat(64), archive_name),
        )
        .unwrap();
    }

    fn test_runtime(root: &Path) -> RuntimePaths {
        RuntimePaths {
            root: root.to_path_buf(),
            plugin_root: root.to_path_buf(),
            worker: root.join("libexec/rzn-phone-worker"),
            workflow_dir: root.join("resources/workflows"),
            systems_dir: root.join("resources/systems"),
            examples_dir: root.join("examples"),
            skills_dir: root.join("skills"),
            version_file: root.join("VERSION"),
            workflow_pack_version_file: root.join("WORKFLOW_PACK_VERSION"),
            update_source_file: root.join("UPDATE_SOURCE"),
        }
    }

    fn seed_installed_runtime(runtime: &RuntimePaths) {
        fs::create_dir_all(&runtime.workflow_dir).unwrap();
        fs::create_dir_all(&runtime.systems_dir).unwrap();
        fs::create_dir_all(&runtime.examples_dir).unwrap();
        fs::write(runtime.workflow_dir.join("old.json"), "{}\n").unwrap();
        fs::write(runtime.systems_dir.join("old.json"), "{}\n").unwrap();
        fs::write(runtime.examples_dir.join("old.json"), "{}\n").unwrap();
        fs::write(&runtime.version_file, "0.0.1\n").unwrap();
        fs::write(&runtime.workflow_pack_version_file, "0.0.1\n").unwrap();
    }

    fn assert_old_install_intact(runtime: &RuntimePaths) {
        assert!(runtime.workflow_dir.join("old.json").is_file());
        assert!(runtime.systems_dir.join("old.json").is_file());
        assert!(runtime.examples_dir.join("old.json").is_file());
        assert!(!runtime.workflow_dir.join("new.json").exists());
    }

    #[tokio::test]
    async fn update_workflows_accepts_valid_local_archive_checksum() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(&tmp.path().join("runtime"));
        seed_installed_runtime(&runtime);
        let source = tmp.path().join("source");
        write_source_archive(&source, "1.2.3", &valid_pack_members("1.2.3"));

        update_workflows(&runtime, Some(source.display().to_string()), None)
            .await
            .unwrap();

        assert!(runtime.workflow_dir.join("new.json").is_file());
        assert!(!runtime.workflow_dir.join("old.json").exists());
        assert!(runtime.systems_dir.join("default.json").is_file());
        assert!(runtime.examples_dir.join("example.json").is_file());
        assert_eq!(
            fs::read_to_string(&runtime.workflow_pack_version_file).unwrap(),
            "1.2.3\n"
        );
        assert_eq!(
            fs::read_to_string(&runtime.update_source_file).unwrap(),
            format!("{}\n", source.display())
        );
    }

    #[tokio::test]
    async fn update_workflows_accepts_file_url_archive_source() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(&tmp.path().join("runtime"));
        seed_installed_runtime(&runtime);
        let source = tmp.path().join("source");
        let archive_path = write_source_archive(&source, "1.2.3", &valid_pack_members("1.2.3"));

        update_workflows(
            &runtime,
            Some(format!("file://{}", archive_path.display())),
            Some("1.2.3".to_string()),
        )
        .await
        .unwrap();

        assert!(runtime.workflow_dir.join("new.json").is_file());
        assert!(!runtime.workflow_dir.join("old.json").exists());
    }

    #[tokio::test]
    async fn update_workflows_rejects_sha256_mismatch_without_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(&tmp.path().join("runtime"));
        seed_installed_runtime(&runtime);
        let source = tmp.path().join("source");
        let archive_path = write_source_archive(&source, "1.2.3", &valid_pack_members("1.2.3"));
        write_mismatched_sha256(&archive_path);

        let err = update_workflows(&runtime, Some(source.display().to_string()), None)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("sha256 mismatch"));
        assert_old_install_intact(&runtime);
    }

    #[test]
    fn remote_signature_rejects_replaced_archive_even_with_matching_sha256() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let archive_path = write_source_archive(&source, "1.2.3", &valid_pack_members("1.2.3"));
        let signing_key = test_signing_key();
        let verifying_key = signing_key.verifying_key();
        write_matching_signature(&archive_path, &signing_key);
        let sig_path = archive_path.with_file_name(format!(
            "{}.sig",
            archive_path.file_name().unwrap().to_str().unwrap()
        ));

        verify_sha256(
            &archive_path,
            &archive_path.with_file_name(format!(
                "{}.sha256",
                archive_path.file_name().unwrap().to_str().unwrap()
            )),
        )
        .unwrap();
        verify_release_signature_with_key(&archive_path, &sig_path, &verifying_key).unwrap();

        write_archive(&archive_path, &valid_pack_members("9.9.9"));
        write_matching_sha256(&archive_path);

        verify_sha256(
            &archive_path,
            &archive_path.with_file_name(format!(
                "{}.sha256",
                archive_path.file_name().unwrap().to_str().unwrap()
            )),
        )
        .unwrap();
        let err =
            verify_release_signature_with_key(&archive_path, &sig_path, &verifying_key).unwrap_err();
        assert!(err.to_string().contains("signature verification failed"));
    }

    #[test]
    fn signature_verification_is_required_only_for_remote_sources() {
        assert!(is_remote_source("https://example.invalid/rzn-phone-workflows-1.2.3.tar.gz"));
        assert!(is_remote_source("http://example.invalid/rzn-phone-workflows-1.2.3.tar.gz"));
        assert!(!is_remote_source("/tmp/rzn-phone-workflows-1.2.3.tar.gz"));
        assert!(!is_remote_source("file:///tmp/rzn-phone-workflows-1.2.3.tar.gz"));
    }

    #[tokio::test]
    async fn update_workflows_rejects_malicious_archive_members_without_mutation() {
        let cases = vec![
            ("absolute", file_member("/tmp/pwned", 0o644, b"bad")),
            (
                "parent_traversal",
                file_member(
                    &format!("{}/../pwned", WORKFLOW_ARCHIVE_ROOT),
                    0o644,
                    b"bad",
                ),
            ),
            (
                "symlink",
                symlink_member(&format!("{}/link", WORKFLOW_ARCHIVE_ROOT), "VERSION"),
            ),
            (
                "hardlink",
                hardlink_member(
                    &format!("{}/hardlink", WORKFLOW_ARCHIVE_ROOT),
                    &format!("{}/VERSION", WORKFLOW_ARCHIVE_ROOT),
                ),
            ),
            (
                "special_file",
                fifo_member(&format!("{}/fifo", WORKFLOW_ARCHIVE_ROOT)),
            ),
            (
                "special_mode_bits",
                file_member(&format!("{}/setuid", WORKFLOW_ARCHIVE_ROOT), 0o4755, b"bad"),
            ),
            (
                "world_writable",
                file_member(
                    &format!("{}/world-writable", WORKFLOW_ARCHIVE_ROOT),
                    0o666,
                    b"bad",
                ),
            ),
        ];

        for (label, bad_member) in cases {
            let tmp = tempfile::tempdir().unwrap();
            let runtime = test_runtime(&tmp.path().join("runtime"));
            seed_installed_runtime(&runtime);
            let source = tmp.path().join("source");
            let mut members = valid_pack_members("1.2.3");
            members.push(bad_member);
            write_source_archive(&source, "1.2.3", &members);

            let err = update_workflows(
                &runtime,
                Some(source.display().to_string()),
                Some("1.2.3".to_string()),
            )
            .await
            .unwrap_err();

            assert!(
                err.to_string().contains("archive")
                    || err.to_string().contains("unsafe")
                    || err.to_string().contains("world-writable"),
                "{} returned unexpected error: {}",
                label,
                err
            );
            assert_old_install_intact(&runtime);
        }
    }

    #[tokio::test]
    async fn update_workflows_rejects_plaintext_http_sources_without_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(&tmp.path().join("runtime"));
        seed_installed_runtime(&runtime);

        let err = update_workflows(
            &runtime,
            Some("http://example.invalid/workflows".to_string()),
            Some("1.2.3".to_string()),
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("plaintext HTTP"));
        assert_old_install_intact(&runtime);
    }

    #[test]
    fn update_workflows_private_tempdirs_are_unique_and_cleaned() {
        let first_path;
        {
            let first = create_private_tempdir().unwrap();
            let second = create_private_tempdir().unwrap();
            first_path = first.path().to_path_buf();
            assert_ne!(first.path(), second.path());
            assert!(first.path().is_dir());
            assert!(second.path().is_dir());
            #[cfg(unix)]
            {
                assert_eq!(
                    first.path().metadata().unwrap().permissions().mode() & 0o777,
                    0o700
                );
                assert_eq!(
                    second.path().metadata().unwrap().permissions().mode() & 0o777,
                    0o700
                );
            }
        }
        assert!(!first_path.exists());
    }
}
