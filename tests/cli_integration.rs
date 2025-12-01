use assert_cmd::{cargo::cargo_bin_cmd, Command};
use predicates::str::contains;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn test_environment(base: &TempDir) -> HashMap<&'static str, String> {
    let home = base.path().join("home");
    let config = home.join(".config");
    let data = home.join(".local").join("share");

    fs::create_dir_all(&config).expect("create config dir");
    fs::create_dir_all(&data).expect("create data dir");

    HashMap::from([
        ("HOME", home.to_string_lossy().to_string()),
        ("XDG_CONFIG_HOME", config.to_string_lossy().to_string()),
        ("XDG_DATA_HOME", data.to_string_lossy().to_string()),
    ])
}

fn apply_env(cmd: &mut Command, envs: &HashMap<&str, String>) {
    for (key, value) in envs {
        cmd.env(key, value);
    }
}

fn create_sample_files(root: &PathBuf) {
    fs::create_dir_all(root).expect("create root dir");
    fs::write(root.join("notes.txt"), "rust search tools").expect("write notes.txt");
    fs::write(root.join("todo.md"), "build fast indexer").expect("write todo.md");
}

#[test]
fn init_index_and_search_flow() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let envs = test_environment(&temp_dir);

    let root = temp_dir.path().join("workspace");
    create_sample_files(&root);

    // Initialize and index the directory
    let mut init_cmd = cargo_bin_cmd!("vaultsearch");
    apply_env(&mut init_cmd, &envs);
    init_cmd
        .args(["init", "--root", root.to_str().unwrap(), "--force"])
        .assert()
        .success()
        .stdout(contains("Initialized vaultsearch"));

    let config_path = PathBuf::from(&envs["XDG_CONFIG_HOME"])
        .join("vaultsearch")
        .join("config.toml");
    assert!(config_path.exists());

    // Add a new file and re-run indexing
    let new_file = root.join("updates.txt");
    fs::write(&new_file, "integration test covers indexing").expect("write updates.txt");

    let mut index_cmd = cargo_bin_cmd!("vaultsearch");
    apply_env(&mut index_cmd, &envs);
    index_cmd
        .arg("index")
        .assert()
        .success()
        .stdout(contains("Indexing complete."));

    // Search for known content from both initial and newly added files
    let mut search_cmd = cargo_bin_cmd!("vaultsearch");
    apply_env(&mut search_cmd, &envs);
    search_cmd
        .args(["search", "rust"])
        .assert()
        .success()
        .stdout(contains("notes.txt"));

    let mut search_new_cmd = cargo_bin_cmd!("vaultsearch");
    apply_env(&mut search_new_cmd, &envs);
    search_new_cmd
        .args(["search", "integration"])
        .assert()
        .success()
        .stdout(contains("updates.txt"));
}
