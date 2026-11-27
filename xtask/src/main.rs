use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::{env, fs};

use makepad_micro_serde::{DeJson, JsonValue};

type Res<T = ()> = Result<T, Box<dyn Error>>;

const DEFAULT_IDENTITY: &str = "Developer ID Application: nekonata, Inc. (F79C2UJ844)";
const DEFAULT_NOTARY: &str = "koma-notary";
const ENTITLEMENTS: &str = "crates/koma-app/macos/entitlements.plist";
const INFO_PLIST: &str = "crates/koma-app/macos/Info.plist";
const RELEASE_FILES: [&str; 4] = ["Cargo.toml", "Cargo.lock", INFO_PLIST, "flake.nix"];

struct Config {
    bundle_target_dir: PathBuf,
    dist_dir: PathBuf,
    app_dir: PathBuf,
    identity: String,
    notary: String,
}

impl Config {
    fn from_env() -> Self {
        let target = PathBuf::from(env_or("CARGO_TARGET_DIR", "target"));
        let dist_dir = target.join("release/dist");
        Self {
            bundle_target_dir: target.join("makepad-bundle"),
            app_dir: dist_dir.join("Koma.app"),
            dist_dir,
            identity: env_or("CODESIGN_IDENTITY", DEFAULT_IDENTITY),
            notary: env_or("NOTARY_PROFILE", DEFAULT_NOTARY),
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.into())
}

fn path_str(path: &Path) -> Res<&str> {
    path.to_str()
        .ok_or_else(|| format!("Non-UTF-8 path: {}", path.display()).into())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let cfg = Config::from_env();
    let res = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["macos"] => cmd_macos(&cfg),
        ["prepare", ver] => cmd_prepare(&cfg, ver),
        ["publish", ver] => cmd_publish(&cfg, ver),
        _ => Err("Usage: xtask <macos|prepare <version>|publish <version>>".into()),
    };
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run(prog: &str, args: &[&str]) -> Res {
    run_env(prog, args, &[])
}

fn run_env(prog: &str, args: &[&str], envs: &[(&str, &str)]) -> Res {
    let status = Command::new(prog)
        .args(args)
        .envs(envs.iter().copied())
        .status()?;
    if !status.success() {
        return Err(format!("Command failed ({status}): {prog} {}", args.join(" ")).into());
    }
    Ok(())
}

fn capture_bytes(prog: &str, args: &[&str]) -> Res<Vec<u8>> {
    let out = Command::new(prog).args(args).output()?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "Command failed ({}): {prog} {}\n{}",
            out.status,
            args.join(" "),
            stderr.trim()
        )
        .into());
    }
    Ok(out.stdout)
}

fn capture(prog: &str, args: &[&str]) -> Res<String> {
    Ok(String::from_utf8(capture_bytes(prog, args)?)?)
}

fn validate_version(ver: &str) -> Res {
    let parts: Vec<&str> = ver.split('.').collect();
    let ok = parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    if !ok {
        return Err(format!("Invalid version: {ver} (expected X.Y.Z)").into());
    }
    Ok(())
}

fn resolve_makepad_root() -> Res<PathBuf> {
    let raw = capture("cargo", &["metadata", "--locked", "--format-version", "1"])?;
    let JsonValue::Object(root) =
        JsonValue::deserialize_json(&raw).map_err(|e| format!("{e:?}"))?
    else {
        return Err("Unexpected cargo metadata root".into());
    };
    let Some(JsonValue::Array(pkgs)) = root.get("packages") else {
        return Err("Missing packages in cargo metadata".into());
    };
    for pkg in pkgs {
        let JsonValue::Object(obj) = pkg else {
            continue;
        };
        if matches!(obj.get("name"), Some(JsonValue::String(n)) if n == "makepad-widgets") {
            let Some(JsonValue::String(manifest)) = obj.get("manifest_path") else {
                break;
            };
            return Path::new(manifest)
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .ok_or_else(|| format!("Unexpected makepad-widgets manifest: {manifest}").into());
        }
    }
    Err("makepad-widgets package not found in cargo metadata".into())
}

fn resolve_cef_framework(makepad_root: &Path) -> Res<PathBuf> {
    if let Ok(dir) = env::var("MAKEPAD_CEF_DIST_DIR") {
        if !dir.is_empty() {
            let fw = Path::new(&dir).join("Release/Chromium Embedded Framework.framework");
            return fw
                .is_dir()
                .then_some(fw)
                .ok_or_else(|| format!("Missing CEF framework in {dir}").into());
        }
    }
    let common_git = capture(
        "git",
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let common_root = Path::new(common_git.trim())
        .parent()
        .ok_or("Cannot resolve git common root")?;
    let mut candidates = vec![
        PathBuf::from("local/cef-prebuilt"),
        common_root.join("local/cef-prebuilt"),
        makepad_root.join("local/cef-prebuilt"),
    ];
    let cargo_home = env::var("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env_or("HOME", "")).join(".cargo"));
    if let Ok(checkouts) = fs::read_dir(cargo_home.join("git/checkouts")) {
        let mut fallback: Vec<PathBuf> = checkouts
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("makepad-"))
            .flat_map(|e| fs::read_dir(e.path()).into_iter().flatten().flatten())
            .map(|rev| rev.path().join("local/cef-prebuilt"))
            .collect();
        fallback.sort();
        candidates.extend(fallback);
    }
    let cef_root = candidates
        .into_iter()
        .find(|d| d.join("current-macosarm64.txt").is_file())
        .ok_or("Missing CEF prebuilt (run 'just cef-fetch' first)")?;
    let current = fs::read_to_string(cef_root.join("current-macosarm64.txt"))?;
    let fw = cef_root
        .join(current.trim())
        .join("Release/Chromium Embedded Framework.framework");
    if !fw.is_dir() {
        return Err(format!("Missing CEF framework: {}", fw.display()).into());
    }
    Ok(fw)
}

fn extract_font_assets(binary: &[u8]) -> Res<Vec<String>> {
    const HEADER: &[u8] = b"format=makepad.font-assets.v1\n";
    let mut assets = Vec::new();
    let mut offset = 0;
    while let Some(pos) = binary[offset..]
        .windows(HEADER.len())
        .position(|w| w == HEADER)
    {
        let mut cursor = offset + pos + HEADER.len();
        offset = cursor;
        while cursor < binary.len() {
            let end = binary[cursor..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(binary.len(), |i| cursor + i);
            let Ok(line) = std::str::from_utf8(&binary[cursor..end]) else {
                break;
            };
            if let Some(asset) = line.strip_prefix("asset=") {
                if !assets.iter().any(|a| a == asset) {
                    assets.push(asset.to_string());
                }
            } else if !line.starts_with("set=") {
                break;
            }
            if end == binary.len() {
                break;
            }
            cursor = end + 1;
        }
    }
    if assets.is_empty() {
        return Err("Missing font asset manifest in koma-app binary".into());
    }
    Ok(assets)
}

fn copy_mode(src: &Path, dst: &Path, mode: u32) -> Res {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dst)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dst, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

fn collect_dylibs(dir: &Path, out: &mut Vec<PathBuf>) -> Res {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            collect_dylibs(&path, out)?;
        } else if ft.is_file() && path.extension().and_then(|e| e.to_str()) == Some("dylib") {
            out.push(path);
        }
    }
    Ok(())
}

fn codesign(identity: &str, entitlements: Option<&str>, target: &Path) -> Res {
    let mut args = vec!["--force"];
    if identity == "-" {
        args.extend(["--sign", "-"]);
    } else {
        args.extend(["--options", "runtime", "--timestamp"]);
        if let Some(ent) = entitlements {
            args.extend(["--entitlements", ent]);
        }
        args.extend(["--sign", identity]);
    }
    args.push(path_str(target)?);
    run("codesign", &args)
}

fn cmd_macos(cfg: &Config) -> Res {
    let makepad_root = resolve_makepad_root()?;
    let widgets_res = makepad_root.join("widgets/resources");
    if !widgets_res.is_dir() {
        return Err(format!(
            "Missing makepad-widgets resources: {}",
            widgets_res.display()
        )
        .into());
    }
    let cef_dist = resolve_cef_framework(&makepad_root)?;

    // MAKEPAD_PACKAGE_DIR=resources はコンパイル時に埋め込まれるため通常ビルドと target を分ける
    let build_args = [
        "build",
        "--release",
        "--locked",
        "--target",
        "aarch64-apple-darwin",
        "-p",
        "koma-app",
        "-p",
        "koma-cli",
        "-p",
        "koma-browser-host",
    ];
    run_env(
        "cargo",
        &build_args,
        &[
            ("CARGO_TARGET_DIR", path_str(&cfg.bundle_target_dir)?),
            ("MAKEPAD_PACKAGE_DIR", "resources"),
        ],
    )?;
    let arch_bin = cfg.bundle_target_dir.join("aarch64-apple-darwin/release");

    if cfg.app_dir.exists() {
        fs::remove_dir_all(&cfg.app_dir)?;
    }
    let contents = cfg.app_dir.join("Contents");
    let macos_dir = contents.join("MacOS");
    let resources_dir = contents.join("Resources");
    let widgets_dst = resources_dir.join("makepad_widgets/resources");
    let frameworks = contents.join("Frameworks");
    for dir in [&macos_dir, &widgets_dst, &frameworks] {
        fs::create_dir_all(dir)?;
    }

    copy_mode(Path::new(INFO_PLIST), &contents.join("Info.plist"), 0o644)?;
    fs::write(contents.join("PkgInfo"), "APPL????")?;
    for bin in ["koma-app", "koma", "makepad-app-browser-host"] {
        copy_mode(&arch_bin.join(bin), &macos_dir.join(bin), 0o755)?;
    }

    run(
        "ditto",
        &[
            path_str(&widgets_res.join("icons"))?,
            path_str(&widgets_dst.join("icons"))?,
        ],
    )?;
    for asset in extract_font_assets(&fs::read(macos_dir.join("koma-app"))?)? {
        let rel = asset
            .strip_prefix("makepad_widgets/resources/")
            .ok_or_else(|| format!("Unexpected font asset: {asset}"))?;
        copy_mode(&widgets_res.join(rel), &widgets_dst.join(rel), 0o644)?;
    }
    fs::write(
        resources_dir.join("koma-app.makepad-package-paths"),
        "makepad_widgets\tresources/makepad_widgets\n",
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink("../Resources", macos_dir.join("resources"))?;
        symlink(
            "../Resources/koma-app.makepad-package-paths",
            macos_dir.join("koma-app.makepad-package-paths"),
        )?;
    }

    let cef_dst = frameworks.join("Chromium Embedded Framework.framework");
    run("ditto", &[path_str(&cef_dist)?, path_str(&cef_dst)?])?;

    let helpers = [
        ("", ".helper"),
        (" (GPU)", ".helper.gpu"),
        (" (Plugin)", ".helper.plugin"),
        (" (Renderer)", ".helper.renderer"),
    ];
    for (suffix, id_suffix) in helpers {
        let name = format!("makepad-app-browser-host Helper{suffix}");
        let hc = frameworks.join(format!("{name}.app/Contents"));
        fs::create_dir_all(hc.join("MacOS"))?;
        fs::create_dir_all(hc.join("Resources"))?;
        copy_mode(
            &arch_bin.join("makepad-app-browser-host"),
            &hc.join("MacOS").join(&name),
            0o755,
        )?;
        fs::write(hc.join("PkgInfo"), "APPL????")?;
        fs::write(hc.join("Info.plist"), helper_plist(&name, id_suffix))?;
    }

    let mut dylibs = Vec::new();
    collect_dylibs(&cef_dst, &mut dylibs)?;
    for lib in &dylibs {
        codesign(&cfg.identity, None, lib)?;
    }
    codesign(&cfg.identity, None, &cef_dst)?;
    for (suffix, _) in helpers {
        codesign(
            &cfg.identity,
            Some(ENTITLEMENTS),
            &frameworks.join(format!("makepad-app-browser-host Helper{suffix}.app")),
        )?;
    }
    codesign(&cfg.identity, None, &macos_dir.join("koma"))?;
    codesign(
        &cfg.identity,
        Some(ENTITLEMENTS),
        &macos_dir.join("makepad-app-browser-host"),
    )?;
    codesign(&cfg.identity, Some(ENTITLEMENTS), &cfg.app_dir)?;
    run(
        "codesign",
        &["--verify", "--deep", "--strict", path_str(&cfg.app_dir)?],
    )
}

fn helper_plist(name: &str, id_suffix: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"https://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \x20   <key>CFBundleExecutable</key>\n    <string>{name}</string>\n\
         \x20   <key>CFBundleIdentifier</key>\n    <string>dev.nekonata.koma{id_suffix}</string>\n\
         \x20   <key>CFBundleName</key>\n    <string>{name}</string>\n\
         \x20   <key>CFBundlePackageType</key>\n    <string>APPL</string>\n\
         \x20   <key>LSBackgroundOnly</key>\n    <true/>\n\
         \x20   <key>LSUIElement</key>\n    <true/>\n\
         </dict>\n</plist>\n"
    )
}

fn require_main_branch() -> Res {
    if capture("git", &["branch", "--show-current"])?.trim() != "main" {
        return Err("Must be on main branch".into());
    }
    Ok(())
}

fn update_cargo_version(content: &str, ver: &str) -> Res<String> {
    let (mut out, mut in_pkg, mut updated) = (String::with_capacity(content.len()), false, false);
    for line in content.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_pkg = trimmed == "[workspace.package]";
        } else if in_pkg && trimmed.starts_with("version = \"") {
            let nl = if line.ends_with('\n') { "\n" } else { "" };
            out.push_str(&format!("version = \"{ver}\"{nl}"));
            updated = true;
            continue;
        }
        out.push_str(line);
    }
    if !updated {
        return Err("Failed to find [workspace.package] version in Cargo.toml".into());
    }
    Ok(out)
}

fn update_flake_nix(content: &str, ver: &str, sri: &str) -> Res<String> {
    let (mut out, mut ver_done, mut hash_done) =
        (String::with_capacity(content.len()), false, false);
    for line in content.split_inclusive('\n') {
        let indent_len = line.len() - line.trim_start().len();
        let (indent, rest) = line.split_at(indent_len);
        let nl = if line.ends_with('\n') { "\n" } else { "" };
        if !ver_done && rest.starts_with("version = \"") && rest.trim_end().ends_with("\";") {
            out.push_str(&format!("{indent}version = \"{ver}\";{nl}"));
            ver_done = true;
        } else if !hash_done && rest.starts_with("hash = \"") && rest.trim_end().ends_with("\";") {
            out.push_str(&format!("{indent}hash = \"{sri}\";{nl}"));
            hash_done = true;
        } else {
            out.push_str(line);
        }
    }
    if !ver_done || !hash_done {
        return Err("Failed to update version or hash in flake.nix".into());
    }
    Ok(out)
}

fn sri_hash(path: &Path) -> Res<String> {
    let digest = capture_bytes("openssl", &["dgst", "-sha256", "-binary", path_str(path)?])?;
    if digest.len() != 32 {
        return Err(format!("Unexpected SHA-256 digest length: {}", digest.len()).into());
    }
    let mut child = Command::new("openssl")
        .args(["base64", "-A"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("Missing openssl stdin")?
        .write_all(&digest)?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(format!("openssl base64 failed ({})", out.status).into());
    }
    Ok(format!("sha256-{}", String::from_utf8(out.stdout)?.trim()))
}

fn validate_publish_status(porcelain: &str) -> Res {
    for line in porcelain.lines().filter(|l| !l.is_empty()) {
        let file = line.get(3..).unwrap_or("");
        if !RELEASE_FILES.contains(&file) {
            return Err(format!("Unexpected working tree change: {line}").into());
        }
    }
    Ok(())
}

fn zip_app(app: &Path, zip: &Path) -> Res {
    let _ = fs::remove_file(zip);
    run(
        "ditto",
        &[
            "-c",
            "-k",
            "--sequesterRsrc",
            "--keepParent",
            path_str(app)?,
            path_str(zip)?,
        ],
    )
}

fn plist_buddy(cmd: &str) -> Res<String> {
    capture("/usr/libexec/PlistBuddy", &["-c", cmd, INFO_PLIST])
}

fn cmd_prepare(cfg: &Config, ver: &str) -> Res {
    validate_version(ver)?;
    require_main_branch()?;
    if !capture("git", &["status", "--porcelain"])?
        .trim()
        .is_empty()
    {
        return Err("Working tree is not clean".into());
    }
    if cfg.identity == "-" {
        return Err("Set Developer ID CODESIGN_IDENTITY before prepare".into());
    }

    fs::write(
        "Cargo.toml",
        update_cargo_version(&fs::read_to_string("Cargo.toml")?, ver)?,
    )?;
    plist_buddy(&format!("Set :CFBundleShortVersionString {ver}"))?;
    plist_buddy(&format!("Set :CFBundleVersion {ver}"))?;
    capture("cargo", &["metadata", "--offline", "--format-version", "1"])?;

    run("just", &["ci"])?;
    cmd_macos(cfg)?;

    let zip = cfg.dist_dir.join(format!("Koma-{ver}-arm64.zip"));
    let app_str = path_str(&cfg.app_dir)?;
    zip_app(&cfg.app_dir, &zip)?;
    run(
        "xcrun",
        &[
            "notarytool",
            "submit",
            path_str(&zip)?,
            "--keychain-profile",
            &cfg.notary,
            "--wait",
        ],
    )?;
    run("xcrun", &["stapler", "staple", app_str])?;
    run("xcrun", &["stapler", "validate", app_str])?;
    run(
        "spctl",
        &["--assess", "--type", "execute", "--verbose=2", app_str],
    )?;
    zip_app(&cfg.app_dir, &zip)?;

    let sri = sri_hash(&zip)?;
    fs::write(
        "flake.nix",
        update_flake_nix(&fs::read_to_string("flake.nix")?, ver, &sri)?,
    )?;
    println!("Prepared: {}", zip.display());
    println!("SRI hash: {sri}");
    Ok(())
}

fn cmd_publish(cfg: &Config, ver: &str) -> Res {
    validate_version(ver)?;
    require_main_branch()?;
    let tag = format!("v{ver}");
    let zip = cfg.dist_dir.join(format!("Koma-{ver}-arm64.zip"));
    if !zip.is_file() {
        return Err(format!(
            "Missing {} (run 'just release prepare {ver}' first)",
            zip.display()
        )
        .into());
    }
    if Command::new("git")
        .args(["rev-parse", "-q", "--verify", &format!("refs/tags/{tag}")])
        .output()?
        .status
        .success()
    {
        return Err(format!("Tag {tag} already exists").into());
    }
    validate_publish_status(&capture(
        "git",
        &["status", "--porcelain", "--untracked-files=all"],
    )?)?;

    let cargo_toml = fs::read_to_string("Cargo.toml")?;
    if update_cargo_version(&cargo_toml, ver)? != cargo_toml {
        return Err(format!("Cargo.toml version does not match {ver}").into());
    }
    for key in ["CFBundleShortVersionString", "CFBundleVersion"] {
        if plist_buddy(&format!("Print :{key}"))?.trim() != ver {
            return Err(format!("Info.plist {key} does not match {ver}").into());
        }
    }
    let sri = sri_hash(&zip)?;
    let flake = fs::read_to_string("flake.nix")?;
    if update_flake_nix(&flake, ver, &sri)? != flake {
        return Err(format!("flake.nix version or hash does not match {ver} / {sri}").into());
    }

    run(
        "git",
        &["add", "Cargo.toml", "Cargo.lock", INFO_PLIST, "flake.nix"],
    )?;
    run(
        "git",
        &["commit", "-m", &format!("chore: {tag} をリリースする")],
    )?;
    run("git", &["tag", &tag])?;
    run("git", &["push", "origin", "main", "--tags"])?;
    run(
        "gh",
        &[
            "release",
            "create",
            &tag,
            path_str(&zip)?,
            "--title",
            &format!("Koma {tag}"),
            "--generate-notes",
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_semver_and_publish_status() {
        assert!(validate_version("0.1.0").is_ok());
        assert!(validate_version("v0.1.0").is_err());
        assert!(validate_version("0.1").is_err());
        assert!(validate_publish_status(" M Cargo.toml\n M flake.nix\n").is_ok());
        assert!(validate_publish_status("?? stray.txt\n").is_err());
    }

    #[test]
    fn extracts_font_assets() {
        let payload = b"\x00format=makepad.font-assets.v1\nset=System\nasset=makepad_widgets/resources/A.ttf\nasset=makepad_widgets/resources/B.ttf\n\x00";
        assert_eq!(
            extract_font_assets(payload).unwrap(),
            vec![
                "makepad_widgets/resources/A.ttf",
                "makepad_widgets/resources/B.ttf"
            ]
        );
        assert!(extract_font_assets(b"none").is_err());
    }

    #[test]
    fn updates_cargo_and_flake() {
        let cargo = "[workspace]\n[workspace.package]\nversion = \"0.1.0\"\n";
        assert_eq!(
            update_cargo_version(cargo, "0.2.0").unwrap(),
            "[workspace]\n[workspace.package]\nversion = \"0.2.0\"\n"
        );
        assert!(update_cargo_version("[workspace]\n", "0.2.0").is_err());

        let flake = "      version = \"0.1.0\";\n      hash = \"sha256-old=\";\n";
        assert_eq!(
            update_flake_nix(flake, "0.2.0", "sha256-new=").unwrap(),
            "      version = \"0.2.0\";\n      hash = \"sha256-new=\";\n"
        );
        assert!(update_flake_nix("version = \"0.1.0\";\n", "0.2.0", "sha256-new=").is_err());
    }
}
