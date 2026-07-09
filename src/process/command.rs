//! Build a ready-to-spawn [`Command`] from a [`ServerConfig`]: shell-wrapped so
//! PATH and `.cmd` shims resolve, with the working directory and layered
//! environment applied.
#![allow(dead_code)] // Consumed by the spawner in 3c.

use crate::model::{EnvVar, ServerConfig};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

/// Wrap a user command string in a shell, so PATH, `.cmd` shims, and shell
/// operators resolve the way a developer expects.
///
/// `shell_override` (from `ServerConfig::shell`) lets a server pick its own
/// shell + flags, e.g. `zsh -lic` to source `.zshrc` for nvm-managed tools.
/// When `None`:
/// - Unix: the login shell (`$SHELL`, else `/bin/sh`) with `-lc "<command>"`,
///   which sources login profiles (`.zprofile`/`.profile`). Caveat: `-lc` is
///   non-interactive and does NOT source `.zshrc`/`.bashrc`, so nvm PATH placed
///   there is not seen — set a per-server `shell` (e.g. `zsh -lic`) for that.
/// - Windows: `cmd /C "<command>"`, so `.cmd` shims like `npm.cmd` resolve.
pub fn shell_invocation(command: &str, shell_override: Option<&str>) -> (OsString, Vec<OsString>) {
    if let Some(spec) = shell_override {
        let mut parts: Vec<&str> = spec.split_whitespace().collect();
        if !parts.is_empty() {
            let program = OsString::from(parts.remove(0));
            let mut args: Vec<OsString> = parts.into_iter().map(OsString::from).collect();
            args.push(OsString::from(command));
            return (program, args);
        }
    }

    #[cfg(windows)]
    let invocation = (
        OsString::from("cmd"),
        vec![OsString::from("/C"), OsString::from(command)],
    );
    #[cfg(unix)]
    let invocation = {
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/sh"));
        (shell, vec![OsString::from("-lc"), OsString::from(command)])
    };
    invocation
}

/// Merge the environment layers into a final key→value map. Precedence
/// (last wins): `.env` file < inline overrides < the configured `PORT`.
/// The parent process environment is inherited separately by `Command` and is
/// not represented here — these entries override it.
pub fn merge_env(
    file_vars: &[(String, String)],
    inline: &[EnvVar],
    port: Option<u16>,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    for (key, value) in file_vars {
        env.insert(key.clone(), value.clone());
    }
    for var in inline {
        env.insert(var.key.clone(), var.value.clone());
    }
    if let Some(port) = port {
        // Inject both conventions so the port field actually takes effect:
        // PORT (Node/Next/Express) and SERVER_PORT (Spring Boot).
        let value = port.to_string();
        env.insert("PORT".to_string(), value.clone());
        env.insert("SERVER_PORT".to_string(), value);
    }
    env
}

/// Parse a `.env` file into ordered key/value pairs WITHOUT mutating this
/// process's environment (dotenvy's iterator API, not `from_path`).
pub fn load_env_file(path: &Path) -> std::io::Result<Vec<(String, String)>> {
    let iter = dotenvy::from_path_iter(path)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    let mut vars = Vec::new();
    for item in iter {
        let (key, value) =
            item.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        vars.push((key, value));
    }
    Ok(vars)
}

/// Build a ready-to-spawn [`Command`] for `config`: the shell-wrapped command,
/// its working directory, and the merged environment (the parent env is
/// inherited; these entries override it). Reads `env_file` if set.
pub fn build_command(config: &ServerConfig) -> std::io::Result<Command> {
    let file_vars = match &config.env_file {
        Some(path) => load_env_file(path)?,
        None => Vec::new(),
    };
    let env = merge_env(&file_vars, &config.env, config.port);

    let (program, args) = shell_invocation(&config.command, config.shell.as_deref());
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.current_dir(&config.cwd);
    for (key, value) in env {
        cmd.env(key, value);
    }
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Preset;

    #[cfg(unix)]
    #[test]
    fn unix_default_wraps_in_login_shell() {
        let (program, args) = shell_invocation("npm run dev", None);
        assert!(!program.is_empty());
        assert_eq!(
            args,
            vec![OsString::from("-lc"), OsString::from("npm run dev")]
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_default_wraps_in_cmd() {
        let (program, args) = shell_invocation("npm run dev", None);
        assert_eq!(program, OsString::from("cmd"));
        assert_eq!(
            args,
            vec![OsString::from("/C"), OsString::from("npm run dev")]
        );
    }

    #[test]
    fn shell_override_parsed_into_program_and_flags() {
        let (program, args) = shell_invocation("npm run dev", Some("zsh -lic"));
        assert_eq!(program, OsString::from("zsh"));
        assert_eq!(
            args,
            vec![OsString::from("-lic"), OsString::from("npm run dev")]
        );
    }

    #[test]
    fn blank_shell_override_falls_back_to_default() {
        // A whitespace-only override must not become an empty program.
        let (program, _args) = shell_invocation("echo hi", Some("   "));
        assert!(!program.is_empty());
    }

    #[test]
    fn inline_overrides_file_and_port_is_authoritative() {
        let file = vec![
            ("PORT".to_string(), "1111".to_string()),
            ("A".to_string(), "from-file".to_string()),
        ];
        let inline = vec![
            EnvVar {
                key: "A".into(),
                value: "from-inline".into(),
            },
            EnvVar {
                key: "B".into(),
                value: "b".into(),
            },
        ];
        let env = merge_env(&file, &inline, Some(8080));
        assert_eq!(env.get("A").map(String::as_str), Some("from-inline"));
        assert_eq!(env.get("B").map(String::as_str), Some("b"));
        assert_eq!(env.get("PORT").map(String::as_str), Some("8080"));
        assert_eq!(env.get("SERVER_PORT").map(String::as_str), Some("8080"));
    }

    #[test]
    fn without_config_port_inline_port_survives() {
        let inline = vec![EnvVar {
            key: "PORT".into(),
            value: "3000".into(),
        }];
        let env = merge_env(&[], &inline, None);
        assert_eq!(env.get("PORT").map(String::as_str), Some("3000"));
        assert_eq!(env.get("SERVER_PORT"), None); // not injected without a configured port
    }

    #[test]
    fn load_env_file_parses_pairs_and_skips_comments() {
        let dir = std::env::temp_dir().join(format!("campfire-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".env");
        std::fs::write(&path, "FOO=bar\n# a comment\nBAZ=qux\n").unwrap();
        let vars = load_env_file(&path).unwrap();
        assert!(vars.contains(&("FOO".to_string(), "bar".to_string())));
        assert!(vars.contains(&("BAZ".to_string(), "qux".to_string())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_command_sets_cwd_env_and_args() {
        let mut config = ServerConfig::from_preset("api", "/srv/api", Preset::NextJs);
        config.command = "npm run dev".into();
        config.port = Some(3000);
        config.env.push(EnvVar {
            key: "NODE_ENV".into(),
            value: "development".into(),
        });

        let cmd = build_command(&config).unwrap();
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/srv/api")));

        let envs: BTreeMap<String, String> = cmd
            .get_envs()
            .filter_map(|(k, v)| Some((k.to_str()?.to_string(), v?.to_str()?.to_string())))
            .collect();
        assert_eq!(envs.get("PORT").map(String::as_str), Some("3000"));
        assert_eq!(
            envs.get("NODE_ENV").map(String::as_str),
            Some("development")
        );

        let args: Vec<OsString> = cmd.get_args().map(|a| a.to_owned()).collect();
        assert!(args.contains(&OsString::from("npm run dev")));
    }
}
