use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    pub user: String,
    pub host: String,
    pub port: u16,
    pub identity_file: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SshError {
    #[error("ssh user cannot be empty")]
    EmptyUser,
    #[error("ssh host cannot be empty")]
    EmptyHost,
}

impl SshTarget {
    pub fn validate(&self) -> Result<(), SshError> {
        if self.user.trim().is_empty() {
            return Err(SshError::EmptyUser);
        }
        if self.host.trim().is_empty() {
            return Err(SshError::EmptyHost);
        }
        Ok(())
    }

    pub fn destination(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }
}

pub fn build_ssh_command(
    target: &SshTarget,
    remote_command: &str,
) -> Result<Vec<String>, SshError> {
    target.validate()?;

    let mut cmd = vec!["ssh".to_string(), "-p".to_string(), target.port.to_string()];

    if let Some(identity_file) = &target.identity_file {
        if !identity_file.trim().is_empty() {
            cmd.push("-i".to_string());
            cmd.push(identity_file.clone());
        }
    }

    cmd.push(target.destination());
    if !remote_command.trim().is_empty() {
        cmd.push(remote_command.to_string());
    }

    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_minimal_ssh_command() {
        let target = SshTarget {
            user: "ubuntu".to_string(),
            host: "example.local".to_string(),
            port: 22,
            identity_file: None,
        };

        let out = build_ssh_command(&target, "whoami").expect("must build");
        assert_eq!(
            out,
            vec!["ssh", "-p", "22", "ubuntu@example.local", "whoami"]
        );
    }

    #[test]
    fn validates_required_fields() {
        let target = SshTarget {
            user: "".to_string(),
            host: "example.local".to_string(),
            port: 22,
            identity_file: None,
        };

        let err = build_ssh_command(&target, "").expect_err("must fail");
        assert_eq!(err, SshError::EmptyUser);
    }
}
