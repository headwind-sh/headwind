use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// HelmRelease is a Flux CD custom resource for managing Helm releases
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "helm.toolkit.fluxcd.io",
    version = "v2",
    kind = "HelmRelease",
    namespaced
)]
#[kube(status = "HelmReleaseStatus")]
#[serde(rename_all = "camelCase")]
pub struct HelmReleaseSpec {
    /// Chart defines the Helm chart to be installed
    pub chart: HelmChartTemplate,

    /// Interval at which to reconcile the Helm release
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,

    /// Values holds the values for this Helm release
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<serde_json::Value>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelmChartTemplate {
    /// Spec holds the template for the HelmChart that will be created
    pub spec: HelmChartTemplateSpec,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelmChartTemplateSpec {
    /// Chart is the name of the chart
    pub chart: String,

    /// Version is the chart version semver expression (defaults to *)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// SourceRef is the reference to the Source the chart is available at
    pub source_ref: SourceReference,

    /// Interval at which to check the Source for updates
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceReference {
    /// Kind of the referent (HelmRepository, GitRepository, Bucket)
    pub kind: String,

    /// Name of the referent
    pub name: String,

    /// Namespace of the referent (defaults to the HelmRelease namespace)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

/// Simplified status condition for HelmRelease
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelmCondition {
    /// Type of the condition
    #[serde(rename = "type")]
    pub condition_type: String,

    /// Status of the condition (True, False, Unknown)
    pub status: String,

    /// Reason for the condition's last transition
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Message providing details about the condition
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelmReleaseStatus {
    /// Conditions holds the conditions for the HelmRelease
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<HelmCondition>>,

    /// LastAppliedRevision is the revision of the last successfully applied source
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_applied_revision: Option<String>,

    /// LastAttemptedRevision is the revision of the last reconciliation attempt
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempted_revision: Option<String>,

    /// ObservedGeneration is the last observed generation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
}
