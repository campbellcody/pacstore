//! Versioned, bounded JSON-lines protocol shared by the GUI and installed helper.
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};

pub const VERSION: u32 = 2;
pub const MAX_MESSAGE: u64 = 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Install { targets: Vec<String> },
    Remove { target: String, cascade: bool },
    Upgrade {},
    CleanOrphans {},
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with(['-', '.'])
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"@._+-".contains(&c))
}
impl Operation {
    pub fn validate(&self) -> Result<(), String> {
        let valid = match self {
            Self::Install { targets } => {
                !targets.is_empty()
                    && targets.len() <= 64
                    && targets.iter().all(|n| {
                        valid_name(n)
                            || n.split_once('/')
                                .is_some_and(|(r, p)| valid_name(r) && valid_name(p))
                    })
            }
            Self::Remove { target, .. } => valid_name(target),
            Self::Upgrade {} | Self::CleanOrphans {} => true,
        };
        if valid {
            Ok(())
        } else {
            Err("Expected 1–64 valid package names, without paths or options.".into())
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Begin { version: u32, operation: Operation },
    Confirm {},
    Answer { choice: usize },
    Cancel {},
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Change {
    pub name: String,
    pub old: Option<String>,
    pub new: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub changes: Vec<Change>,
    pub download_bytes: i64,
    pub disk_delta: i64,
    pub root_free: Option<u64>,
    pub cache_free: Option<u64>,
    pub decisions: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Status {
        message: String,
    },
    Log {
        message: String,
    },
    Progress {
        message: String,
        percent: f32,
    },
    Question {
        message: String,
        choices: Vec<String>,
    },
    Prepared {
        plan: Plan,
    },
    Committing,
    Finished {
        success: bool,
        message: String,
    },
}

pub fn read<T: serde::de::DeserializeOwned>(reader: &mut impl BufRead) -> Result<T, String> {
    let mut line = String::new();
    let count = std::io::Read::take(&mut *reader, MAX_MESSAGE + 1)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if count == 0 {
        return Err("Connection closed".into());
    }
    if count as u64 > MAX_MESSAGE || !line.ends_with('\n') {
        return Err("Protocol message is too large or incomplete".into());
    }
    serde_json::from_str(&line).map_err(|e| format!("Invalid protocol message: {e}"))
}
pub fn write(writer: &mut impl Write, message: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(message).map_err(|e| e.to_string())?;
    if bytes.len() as u64 >= MAX_MESSAGE {
        return Err("Protocol message is too large".into());
    }
    writer
        .write_all(&bytes)
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests_reject_options_unknown_fields_and_oversized_targets() {
        for target in ["--root", "../pkg", "foo;id", "foo bar", ""] {
            assert!(!valid_name(target));
        }
        assert!(valid_name("lib32-libstdc++"));
        assert!(
            serde_json::from_str::<Operation>(r#"{"operation":"upgrade","no_signature":true}"#)
                .is_err()
        );
        assert!(
            Operation::Install {
                targets: vec!["pkg".into(); 65]
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn frames_require_newline_and_bound_memory() {
        assert!(read::<Command>(&mut std::io::Cursor::new(b"{\"type\":\"confirm\"}")).is_err());
        assert!(
            read::<Command>(&mut std::io::Cursor::new(vec![
                b'x';
                MAX_MESSAGE as usize + 2
            ]))
            .is_err()
        );
        assert!(matches!(
            read::<Command>(&mut std::io::Cursor::new(b"{\"type\":\"cancel\"}\n")),
            Ok(Command::Cancel {})
        ));
    }
}
