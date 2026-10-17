//! Validation for in-memory demo operations. The production helper independently
//! validates the bounded protocol defined in `protocol` before using libalpm.
use crate::{
    error::{AppError, Result},
    transaction::Request,
};

pub fn validate(request: &Request) -> Result<()> {
    let name = match request {
        Request::Install(name) | Request::Remove(name) => name,
        Request::Upgrade | Request::CleanOrphans => return Ok(()),
    };
    // A package identity, never a command, path, option, or arbitrary argument.
    if name.is_empty()
        || name.starts_with('-')
        || name.starts_with('.')
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"@._+-".contains(&c))
    {
        return Err(AppError::InvalidRequest("Expected a package name.".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_options_paths_and_shell_syntax() {
        for name in ["", "--root", "../pkg", "foo;id", "foo bar", "$(id)"] {
            assert!(validate(&Request::Install(name.into())).is_err());
        }
        assert!(validate(&Request::Install("lib32-libstdc++".into())).is_ok());
        assert!(validate(&Request::Upgrade).is_ok());
    }
}
