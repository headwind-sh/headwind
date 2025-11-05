use crate::models::{ResourcePolicy, UpdatePolicy};
use anyhow::{Context, Result};
use semver::Version;
use tracing::{debug, info};

pub struct PolicyEngine;

impl PolicyEngine {
    #[allow(dead_code)]
    pub fn should_update(
        &self,
        policy: &ResourcePolicy,
        current_version: &str,
        new_version: &str,
    ) -> Result<bool> {
        match policy.policy {
            UpdatePolicy::None => {
                debug!("Policy is 'none', skipping update");
                Ok(false)
            },
            UpdatePolicy::Force => {
                info!("Policy is 'force', allowing update");
                Ok(true)
            },
            UpdatePolicy::All => {
                info!("Policy is 'all', allowing update");
                Ok(current_version != new_version)
            },
            UpdatePolicy::Glob => {
                if let Some(pattern) = &policy.pattern {
                    let matches = glob_match(pattern, new_version);
                    debug!("Glob pattern '{}' match: {}", pattern, matches);
                    Ok(matches)
                } else {
                    Ok(false)
                }
            },
            UpdatePolicy::Patch | UpdatePolicy::Minor | UpdatePolicy::Major => {
                self.check_semver_policy(policy.policy, current_version, new_version)
            },
        }
    }

    fn check_semver_policy(&self, policy: UpdatePolicy, current: &str, new: &str) -> Result<bool> {
        // Try to parse as semver, stripping common prefixes
        let current_version = Self::parse_version(current)
            .context(format!("Failed to parse current version: {}", current))?;
        let new_version =
            Self::parse_version(new).context(format!("Failed to parse new version: {}", new))?;

        if new_version <= current_version {
            debug!(
                "New version {} is not greater than current version {}",
                new, current
            );
            return Ok(false);
        }

        let should_update = match policy {
            UpdatePolicy::Patch => {
                // Only update if major and minor are the same
                new_version.major == current_version.major
                    && new_version.minor == current_version.minor
            },
            UpdatePolicy::Minor => {
                // Update if major is the same
                new_version.major == current_version.major
            },
            UpdatePolicy::Major => {
                // Update to any newer version
                true
            },
            _ => false,
        };

        info!(
            "Semver policy {:?}: current={}, new={}, should_update={}",
            policy, current, new, should_update
        );

        Ok(should_update)
    }

    fn parse_version(version: &str) -> Result<Version> {
        // Strip common prefixes like 'v' or 'release-'
        let clean = version
            .trim_start_matches('v')
            .trim_start_matches("release-")
            .trim();

        Version::parse(clean).context("Invalid semver version")
    }
}

#[allow(dead_code)]
fn glob_match(pattern: &str, text: &str) -> bool {
    // Simple glob matching - can be enhanced with a proper glob library
    if pattern == "*" {
        return true;
    }

    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];
            return text.starts_with(prefix) && text.ends_with(suffix);
        }
    }

    pattern == text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_policy() {
        let engine = PolicyEngine;
        let policy = ResourcePolicy {
            policy: UpdatePolicy::Patch,
            ..Default::default()
        };

        // Should update patch version
        assert!(engine.should_update(&policy, "1.2.3", "1.2.4").unwrap());

        // Should not update minor version
        assert!(!engine.should_update(&policy, "1.2.3", "1.3.0").unwrap());

        // Should not update major version
        assert!(!engine.should_update(&policy, "1.2.3", "2.0.0").unwrap());
    }

    #[test]
    fn test_minor_policy() {
        let engine = PolicyEngine;
        let policy = ResourcePolicy {
            policy: UpdatePolicy::Minor,
            ..Default::default()
        };

        // Should update patch version
        assert!(engine.should_update(&policy, "1.2.3", "1.2.4").unwrap());

        // Should update minor version
        assert!(engine.should_update(&policy, "1.2.3", "1.3.0").unwrap());

        // Should not update major version
        assert!(!engine.should_update(&policy, "1.2.3", "2.0.0").unwrap());
    }

    #[test]
    fn test_major_policy() {
        let engine = PolicyEngine;
        let policy = ResourcePolicy {
            policy: UpdatePolicy::Major,
            ..Default::default()
        };

        // Should update all versions
        assert!(engine.should_update(&policy, "1.2.3", "1.2.4").unwrap());
        assert!(engine.should_update(&policy, "1.2.3", "1.3.0").unwrap());
        assert!(engine.should_update(&policy, "1.2.3", "2.0.0").unwrap());
    }

    #[test]
    fn test_version_parsing_with_prefix() {
        let engine = PolicyEngine;
        let policy = ResourcePolicy {
            policy: UpdatePolicy::Patch,
            ..Default::default()
        };

        // Should handle 'v' prefix
        assert!(engine.should_update(&policy, "v1.2.3", "v1.2.4").unwrap());
    }

    #[test]
    fn test_glob_matching() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("v1.*", "v1.2.3"));
        assert!(glob_match("*-beta", "v1.0.0-beta"));
        assert!(!glob_match("v1.*", "v2.0.0"));
    }
}
