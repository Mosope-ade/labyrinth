use std::process::Command;
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_laby"))
        .env_clear()
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn help_and_version_need_no_credentials_or_desktop_keyring() {
    let result = run(&["--help"]);
    assert!(result.status.success());
    let help = String::from_utf8(result.stdout).unwrap();
    for command in ["login", "logout", "whoami", "onboard", "project-create"] {
        assert!(help.contains(command));
    }
    assert!(run(&["--version"]).status.success());
}
#[test]
fn bad_inputs_fail_before_network_and_do_not_echo_credentials() {
    let secret = "fake-secret-value";
    let result = run(&[
        "--api-url",
        "http://example.com",
        "--supabase-url",
        "https://project.supabase.co",
        "--publishable-key",
        secret,
        "whoami",
    ]);
    assert!(!result.status.success());
    let output = String::from_utf8(result.stderr).unwrap();
    assert!(!output.contains(secret));
    assert!(output.contains("Invalid client configuration"));
    assert!(!run(&["login", "--callback-port", "0"]).status.success());
    assert!(!run(&["projects", "invalid-uuid"]).status.success());
}
