use crate::{assert_command_succeeded, run};

#[test]
fn help_version_and_error_exit_codes_are_stable() {
    let version = run(&["--version"]);

    assert_command_succeeded("--version", &version);
    assert_eq!(
        String::from_utf8(version.stdout).unwrap(),
        format!("scoutly {}\n", env!("CARGO_PKG_VERSION"))
    );

    let help = run(&["--help"]);
    assert_command_succeeded("--help", &help);
    let help = String::from_utf8(help.stdout).unwrap();

    for flag in [
        "--config",
        "--no-config",
        "--keep-fragments",
        "--no-keep-fragments",
        "--ignore-redirects",
        "--no-ignore-redirects",
        "--respect-robots",
        "--no-respect-robots",
        "--sitemaps",
        "--no-sitemaps",
        "--images",
        "--no-images",
        "--include-path",
        "--exclude-path",
        "--format",
        "--progress",
    ] {
        assert!(help.contains(flag), "Rust help is missing {flag}:\n{help}");
    }

    for (name, args, message, expected_code) in [
        (
            "conflicting config flags",
            vec!["--config=scoutly.toml", "--no-config"],
            "cannot be used with",
            2,
        ),
        (
            "invalid target",
            vec!["--no-config", "--progress=never", "http://:8080"],
            "must be an absolute HTTP(S) URL",
            1,
        ),
        (
            "non-terminal TUI",
            vec!["--no-config"],
            "TUI requires a terminal",
            1,
        ),
    ] {
        let output = run(&args);

        assert_eq!(
            output.status.code(),
            Some(expected_code),
            "{name}: {output:?}"
        );
        assert!(output.stdout.is_empty(), "{name} wrote a report");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains(message), "{name}: {stderr}");
    }
}
