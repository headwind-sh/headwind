use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdatePolicy {
    /// Only update patch versions (1.2.3 -> 1.2.4)
    Patch,
    /// Update minor versions (1.2.3 -> 1.3.0)
    Minor,
    /// Update major versions (1.2.3 -> 2.0.0)
    Major,
    /// Update to any new version
    All,
    /// Match glob pattern
    Glob,
    /// Force update regardless of version
    Force,
    /// Never update automatically
    None,
}

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("Invalid policy: {0}")]
    InvalidPolicy(String),
}

impl FromStr for UpdatePolicy {
    type Err = PolicyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "patch" => Ok(UpdatePolicy::Patch),
            "minor" => Ok(UpdatePolicy::Minor),
            "major" => Ok(UpdatePolicy::Major),
            "all" => Ok(UpdatePolicy::All),
            "glob" => Ok(UpdatePolicy::Glob),
            "force" => Ok(UpdatePolicy::Force),
            "none" => Ok(UpdatePolicy::None),
            _ => Err(PolicyError::InvalidPolicy(s.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePolicy {
    /// Update policy to apply
    pub policy: UpdatePolicy,

    /// Optional glob pattern for matching versions (when policy is Glob)
    pub pattern: Option<String>,

    /// Whether approval is required before updating
    pub require_approval: bool,

    /// Minimum time between updates (in seconds)
    pub min_update_interval: Option<u64>,

    /// Images to track (if empty, track all)
    pub images: Vec<String>,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            policy: UpdatePolicy::None,
            pattern: None,
            require_approval: true,
            min_update_interval: Some(300), // 5 minutes
            images: Vec::new(),
        }
    }
}

/// Annotation keys used on Kubernetes resources
pub mod annotations {
    pub const POLICY: &str = "headwind.sh/policy";
    pub const PATTERN: &str = "headwind.sh/pattern";
    pub const REQUIRE_APPROVAL: &str = "headwind.sh/require-approval";
    pub const MIN_UPDATE_INTERVAL: &str = "headwind.sh/min-update-interval";
    pub const IMAGES: &str = "headwind.sh/images";
    pub const LAST_UPDATE: &str = "headwind.sh/last-update";
}
