use std::collections::HashMap;

use pretty_assertions::assert_eq;
use sudo_test::{Command, Env};

use crate::{helpers, Result, SUDOERS_ROOT_ALL_NOPASSWD};

// NOTE if 'env_reset' is not in `/etc/sudoers` it is enabled by default

// see 'environment' section in`man sudo`
// see 'command environment' section in`man sudoers`
#[test]
fn vars_set_by_sudo_in_env_reset_mode() -> Result<()> {
    let env = Env(SUDOERS_ROOT_ALL_NOPASSWD).build()?;

    let stdout = Command::new("env").exec(&env)?.stdout()?;
    let normal_env = helpers::parse_env_output(&stdout)?;

    let sudo_abs_path = Command::new("which").arg("sudo").exec(&env)?.stdout()?;
    let env_abs_path = Command::new("which").arg("env").exec(&env)?.stdout()?;

    // run sudo in an empty environment
    let stdout = Command::new("env")
        .args([
            "-i",
            "SUDO_RS_IS_UNSTABLE=I accept that my system may break unexpectedly",
            &sudo_abs_path,
            &env_abs_path,
        ])
        .exec(&env)?
        .stdout()?;
    let mut sudo_env = helpers::parse_env_output(&stdout)?;

    // # man sudo
    // "Set to the mail spool of the target user"
    assert_eq!(Some("/var/mail/root"), sudo_env.remove("MAIL"));

    // "Set to the home directory of the target user"
    assert_eq!(Some("/root"), sudo_env.remove("HOME"));

    // "Set to the login name of the target user"
    assert_eq!(Some("root"), sudo_env.remove("LOGNAME"));

    // "Set to the command run by sudo, including any args"
    assert_eq!(Some("/usr/bin/env"), sudo_env.remove("SUDO_COMMAND"));

    // "Set to the group-ID of the user who invoked sudo"
    assert_eq!(Some("0"), sudo_env.remove("SUDO_GID"));

    // "Set to the user-ID of the user who invoked sudo"
    assert_eq!(Some("0"), sudo_env.remove("SUDO_UID"));

    // "Set to the login name of the user who invoked sudo"
    assert_eq!(Some("root"), sudo_env.remove("SUDO_USER"));

    // "Set to the same value as LOGNAME"
    assert_eq!(Some("root"), sudo_env.remove("USER"));

    // # man sudoers
    // "The HOME, MAIL, SHELL, LOGNAME and USER environment variables are initialized based on the target user"
    assert_eq!(Some("/bin/bash"), sudo_env.remove("SHELL"));

    // "If the PATH and TERM variables are not preserved from the user's environment, they will be set to default values."
    let sudo_path = sudo_env.remove("PATH").expect("PATH not set");

    let normal_path = normal_env["PATH"];
    assert_ne!(normal_path, sudo_path);

    let default_path = "/usr/bin:/bin:/usr/sbin:/sbin";
    assert_eq!(default_path, sudo_path);

    let default_term = "unknown";
    assert_eq!(Some(default_term), sudo_env.remove("TERM"));

    let empty = HashMap::new();
    assert_eq!(empty, sudo_env);

    Ok(())
}

// the preceding test tests the case where PATH is unset. this one tests the case where PATH is set
#[test]
fn user_path_remains_unchanged_when_not_unset() -> Result<()> {
    let env = Env(SUDOERS_ROOT_ALL_NOPASSWD).build()?;

    let expected = "/root";

    let actual = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "export PATH={expected}; /usr/bin/sudo /usr/bin/printenv PATH"
        ))
        .exec(&env)?
        .stdout()?;

    assert_eq!(expected, actual);

    Ok(())
}

#[test]
fn env_reset_mode_clears_env_vars() -> Result<()> {
    let env = Env(SUDOERS_ROOT_ALL_NOPASSWD).build()?;

    let varname = "SHOULD_BE_REMOVED";
    let set_env_var = format!("export {varname}=1");

    // sanity check that `set_env_var` makes `varname` visible to `env` program
    let stdout = Command::new("sh")
        .arg("-c")
        .arg(format!("{set_env_var}; env"))
        .exec(&env)?
        .stdout()?;
    let env_vars = helpers::parse_env_output(&stdout)?;
    assert!(env_vars.contains_key(varname));

    let stdout = Command::new("sh")
        .arg("-c")
        .arg(format!("{set_env_var}; sudo env"))
        .exec(&env)?
        .stdout()?;
    let env_vars = helpers::parse_env_output(&stdout)?;
    assert!(!env_vars.contains_key(varname));

    Ok(())
}
