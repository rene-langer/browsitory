//! `test_support::git_command` must not hand the git CLI the developer's SSH agent (#805), even
//! where the hermetic guard didn't run or a test set it later: a twin that signs with SSH would
//! ask that agent for the key. This binary has no `hermetic_git!` and exactly one test, so
//! changing its own environment races with nothing.

#[test]
fn git_command_drops_the_ssh_agent_env() {
    std::env::set_var("SSH_AUTH_SOCK", "/nonexistent/real-agent.sock");
    std::env::set_var("SSH_AGENT_PID", "4242");
    let dir = test_support::paths().home.clone();

    let command = test_support::git_command(&dir);
    for name in ["SSH_AUTH_SOCK", "SSH_AGENT_PID"] {
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == name && value.is_none()),
            "{name} is not removed"
        );
    }

    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    // The environment the git CLI hands its own children (a shell alias, a hook, ssh-keygen).
    let output = test_support::git(&dir, &["-c", "alias.showenv=!env", "showenv"]);
    assert!(output.status.success(), "{output:?}");
    let env = String::from_utf8_lossy(&output.stdout);
    let leaked: Vec<&str> = env
        .lines()
        .filter(|line| line.starts_with("SSH_AUTH_SOCK=") || line.starts_with("SSH_AGENT_PID="))
        .collect();
    assert!(leaked.is_empty(), "leaked: {leaked:?}");
}
