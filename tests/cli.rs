use std::{fs, process::Command};
fn fixture() -> tempfile::TempDir {
    fs::create_dir_all("target/test-data").unwrap();
    tempfile::tempdir_in("target/test-data").unwrap()
}
fn command(temp: &tempfile::TempDir) -> Command {
    let path = temp.path().canonicalize().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_cli-radio"));
    command
        .env("XDG_CONFIG_HOME", path.join("config"))
        .env("XDG_DATA_HOME", path.join("data"))
        .env("XDG_STATE_HOME", path.join("state"));
    // No mpv executable or terminal is available. Help/errors must still exit.
    command.env("PATH", path.join("no-mpv"));
    command
}
#[test]
fn short_and_long_help_are_identical_complete_and_side_effect_free() {
    let temp = fixture();
    let first = command(&temp).arg("-h").output().unwrap();
    let second = command(&temp).arg("--help").output().unwrap();
    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert!(first.stderr.is_empty());
    assert!(!first.stdout.contains(&27));
    let text = String::from_utf8(first.stdout).unwrap();
    for value in [
        "stations.toml",
        "Markus Schneider",
        "r                Toggle",
        "l                Switch",
        "random_interval_hours",
        "12",
        "HTTP_PROXY",
        "NO_PROXY",
        "i                Open Info/Diagnostics",
        "BUFFERING / RELIABILITY",
        "10 seconds",
        "--no-config",
    ] {
        assert!(text.contains(value), "missing {value}");
    }
    assert!(!text.contains("--bootstrap-stations"));
    assert!(!text.contains("--import-defaults"));
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}
#[test]
fn help_does_not_modify_existing_files() {
    let temp = fixture();
    for dir in ["config/cli-radio", "data/cli-radio"] {
        fs::create_dir_all(temp.path().join(dir)).unwrap();
    }
    let config = temp.path().join("config/cli-radio/config.toml");
    let stations = temp.path().join("data/cli-radio/stations.toml");
    fs::write(&config, "volume = 42\n").unwrap();
    fs::write(&stations, "stations = []\n").unwrap();
    assert!(
        command(&temp)
            .arg("--help")
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(fs::read_to_string(config).unwrap(), "volume = 42\n");
    assert_eq!(fs::read_to_string(stations).unwrap(), "stations = []\n");
    assert!(!temp.path().join("state").exists());
}
#[test]
fn unknown_options_and_arguments_fail_before_any_side_effect() {
    let temp = fixture();
    for args in [
        vec!["--foobar"],
        vec!["--import-defaults"],
        vec!["--does-not-exist", "--help"],
        vec!["--help", "--foobar"],
        vec!["arbitrary"],
    ] {
        let output = command(&temp).args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("cli-radio --help")
        );
    }
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}
#[test]
fn version_is_available_without_config_or_terminal() {
    let temp = fixture();
    let output = command(&temp).arg("--version").output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .eq(&format!("cli-radio {}\n", env!("CARGO_PKG_VERSION")))
    );
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn maintainer_option_is_not_publicly_documented() {
    let readme = include_str!("../README.md");
    assert!(!readme.contains("--bootstrap-stations"));
    assert!(!readme.contains("--import-defaults"));
    assert!(!include_str!("../docs/cli-radio.1").contains("bootstrap-stations"));
    assert!(!cli_radio::cli::HELP.contains("--bootstrap-stations"));
}
