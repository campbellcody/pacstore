use crate::{
    error::{AppError, Result},
    helper,
    packages::Package,
};

#[derive(Clone, Debug)]
pub enum Request {
    Install(String),
    Remove(String),
    Upgrade,
    CleanOrphans,
}

/// Operate on an owned catalog in the worker. Only replace UI state on success.
pub fn execute_demo(mut packages: Vec<Package>, request: Request) -> Result<Vec<Package>> {
    helper::validate(&request)?;
    match request {
        Request::Install(ref name) | Request::Remove(ref name) => {
            let package = packages
                .iter_mut()
                .find(|p| p.name == *name)
                .ok_or_else(|| {
                    AppError::InvalidRequest(
                        "Package is no longer in the catalog. Reload and retry.".into(),
                    )
                })?;
            if matches!(request, Request::Install(_)) {
                package.install_reason = "Explicitly installed".into();
            }
            package.installed = if matches!(request, Request::Install(_)) {
                Some(package.version.clone())
            } else {
                None
            };
        }
        Request::CleanOrphans => {
            for p in &mut packages {
                if p.orphan {
                    p.installed = None;
                    p.orphan = false;
                }
            }
        }
        Request::Upgrade => {
            for package in &mut packages {
                if package.has_update() {
                    package.installed = Some(package.version.clone());
                }
            }
        }
    }
    for package in &mut packages {
        package.update_available = package
            .installed
            .as_ref()
            .is_some_and(|v| v != &package.version);
    }
    log::info!("Demo transaction completed; no system changes");
    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_request_is_an_error() {
        assert!(execute_demo(Vec::new(), Request::Remove("missing".into())).is_err());
    }
}
