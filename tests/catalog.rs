//! End-to-end read-only backend checks with isolated pacman executables/databases.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
};

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "pacstore-catalog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for directory in [
            "bin",
            "db/sync",
            "db/local/pkg-1",
            "usr/share/swcatalog/xml",
            "usr/share/applications",
        ] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        let fixture = Self { root };
        fixture.write("db/sync/core.db", "initial");
        fixture.write("db/local/pkg-1/desc", "initial");
        fixture.write("sync.txt", "Repository      : core\nName            : pkg\nVersion         : 2\nDescription     : Package description\nArchitecture    : x86_64\nLicenses        : MIT\nDepends On      : libc\nOptional Deps   : helper: feature\n                  other: more\nDownload Size   : 1.00 MiB\nInstalled Size  : 2.00 MiB\nURL             : https://example.org\n\n");
        fixture.write("local.txt", "Name            : pkg\nVersion         : 1\nInstall Reason  : Explicitly installed\n\n");
        fixture.write("updates.txt", "pkg 1 -> 2\n");
        fixture.write("usr/share/swcatalog/xml/catalog.xml", "<components><component type=\"desktop-application\"><pkgname>pkg</pkgname><name>Friendly App</name><summary>Image editor</summary><categories><category>Graphics</category></categories><screenshots><screenshot><image type=\"source\">https://example.org/screen.png</image></screenshot></screenshots></component></components>");
        fixture.script(
            "pacman-conf",
            &format!(
                r#"#!/bin/sh
case "$*" in
  --repo-list) echo core ;;
  DBPath) echo '{root}/db' ;;
  '') printf 'RootDir = {root}\nDBPath = {root}/db\n[core]\n' ;;
  *) exit 90 ;;
esac
"#,
                root = fixture.root.display()
            ),
        );
        fixture.script(
            "pacman",
            &format!(
                r#"#!/bin/sh
printf '%s\n' "$*" >> '{root}/commands'
case "$*" in
  -Si) /bin/cat '{root}/sync.txt' ;;
  -Qi) /bin/cat '{root}/local.txt' ;;
  -Qdtq) exit 1 ;;
  -Qu) /bin/cat '{root}/updates.txt' ;;
  -Ql) exit 0 ;;
  *) echo 'Unexpected pacman operation' >&2; exit 90 ;;
esac
"#,
                root = fixture.root.display()
            ),
        );
        fixture
    }
    fn write(&self, path: &str, text: &str) {
        fs::write(self.root.join(path), text).unwrap();
    }
    fn script(&self, name: &str, text: &str) {
        let path = self.root.join("bin").join(name);
        fs::write(&path, text).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn run(&self, reload: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pacstore"));
        command
            .arg("--check-catalog")
            .env("PATH", self.root.join("bin"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("XDG_CONFIG_HOME", self.root.join("config"));
        if reload {
            command.arg("--reload");
        }
        command.output().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn live_cache_invalidation_and_offline_fallback() {
    let f = Fixture::new();
    // Exercise compressed AppStream catalogs too.
    let xml = f.root.join("usr/share/swcatalog/xml/catalog.xml");
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut gzip, &fs::read(&xml).unwrap()).unwrap();
    fs::write(xml.with_extension("xml.gz"), gzip.finish().unwrap()).unwrap();
    fs::remove_file(xml).unwrap();
    let first = f.run(false);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        String::from_utf8_lossy(&first.stdout)
            .contains("1 packages, 1 applications, 1 installed, 1 updates")
    );
    let cache_path = f.root.join("cache/pacstore/catalog-v1.json");
    let cache: serde_json::Value = serde_json::from_slice(&fs::read(&cache_path).unwrap()).unwrap();
    let p = &cache["catalog"]["packages"][0];
    assert_eq!(p["application_name"], "Friendly App");
    assert_eq!(p["licenses"], "MIT");
    assert_eq!(p["dependencies"], "libc");
    assert_eq!(p["screenshots"][0], "https://example.org/screen.png");
    let calls = fs::read_to_string(f.root.join("commands")).unwrap();
    let cached = f.run(false);
    assert!(String::from_utf8_lossy(&cached.stdout).contains("Catalog cache"));
    assert_eq!(fs::read_to_string(f.root.join("commands")).unwrap(), calls);

    // An external package/database change invalidates the snapshot.
    f.write("db/local/pkg-1/desc", "different size and timestamp");
    f.write("updates.txt", "");
    assert!(String::from_utf8_lossy(&f.run(false).stdout).contains("0 updates"));
    assert!(fs::read_to_string(f.root.join("commands")).unwrap().len() > calls.len());

    f.write("db/sync/core.db", "repository refreshed with new metadata");
    f.write("updates.txt", "pkg 1 -> 2\n");
    assert!(String::from_utf8_lossy(&f.run(false).stdout).contains("1 updates"));

    // Force reload bypasses a valid cache; corrupt cache also recovers.
    assert!(String::from_utf8_lossy(&f.run(true).stdout).contains("System catalog"));
    fs::write(&cache_path, "invalid JSON").unwrap();
    assert!(f.run(false).status.success());

    // No usable tools: browse the stale snapshot and do not claim updates.
    fs::remove_file(f.root.join("bin/pacman-conf")).unwrap();
    let offline = f.run(false);
    assert!(offline.status.success());
    assert!(String::from_utf8_lossy(&offline.stdout).contains("Offline catalog snapshot"));
    assert!(String::from_utf8_lossy(&offline.stderr).contains("stale"));
    for command in fs::read_to_string(f.root.join("commands")).unwrap().lines() {
        assert!(["-Si", "-Qi", "-Qu", "-Ql", "-Qdtq"].contains(&command));
    }
}

#[test]
fn missing_repository_and_invalid_metadata_keep_installed_packages_visible() {
    let f = Fixture::new();
    f.script(
        "pacman",
        &format!(
            r#"#!/bin/sh
case "$*" in
  -Si) echo 'database missing' >&2; exit 1 ;;
  -Qi) /bin/cat '{root}/local.txt' ;;
  -Qdtq) exit 1 ;;
  -Qu) exit 1 ;;
  -Ql) exit 0 ;;
  *) exit 90 ;;
esac
"#,
            root = f.root.display()
        ),
    );
    f.write("usr/share/swcatalog/xml/catalog.xml", "<invalid");
    let result = f.run(false);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout)
            .contains("1 packages, 0 applications, 1 installed, unknown updates")
    );
    let warnings = String::from_utf8_lossy(&result.stderr);
    assert!(warnings.contains("database missing"));
    assert!(warnings.contains("Skipped 1"));
}
