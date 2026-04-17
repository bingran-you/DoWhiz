use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RuntimeSkillsManifest {
    runtime_root: String,
    shared_skill_dirs: Vec<String>,
    non_skill_dirs: Vec<String>,
}

fn service_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("scheduler_module lives under DoWhiz_service")
        .to_path_buf()
}

fn collect_runtime_skill_dirs(skills_root: &Path) -> (Vec<String>, Vec<String>) {
    let mut shared = Vec::new();
    let mut non_skill = Vec::new();

    for entry in fs::read_dir(skills_root).expect("read skills dir") {
        let entry = entry.expect("skills dir entry");
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if path.join("SKILL.md").exists() {
            shared.push(name);
        } else {
            non_skill.push(name);
        }
    }

    shared.sort();
    non_skill.sort();
    (shared, non_skill)
}

#[test]
fn runtime_skills_manifest_matches_filesystem() {
    let service_root = service_root();
    let manifest_path = service_root.join("skills").join("manifest.toml");
    let manifest: RuntimeSkillsManifest =
        toml::from_str(&fs::read_to_string(&manifest_path).expect("read manifest"))
            .expect("parse manifest");

    assert_eq!(
        manifest.runtime_root, "DoWhiz_service/skills",
        "manifest should document the canonical runtime skills root"
    );

    let skills_root = service_root.join("skills");
    let (shared_skill_dirs, non_skill_dirs) = collect_runtime_skill_dirs(&skills_root);

    assert_eq!(
        manifest.shared_skill_dirs, shared_skill_dirs,
        "shared_skill_dirs should stay in sync with skill directories containing SKILL.md"
    );
    assert_eq!(
        manifest.non_skill_dirs, non_skill_dirs,
        "non_skill_dirs should track directories that exist under skills/ without SKILL.md"
    );
}
