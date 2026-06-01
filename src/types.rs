use std::collections::BTreeMap;
use std::fmt;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Core,
    Agent,
    Data,
    Multimodal,
    Full,
    Destructive,
}

impl Profile {
    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Core => "core",
            Profile::Agent => "agent",
            Profile::Data => "data",
            Profile::Multimodal => "multimodal",
            Profile::Full => "full",
            Profile::Destructive => "destructive",
        }
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    Terminal,
    Json,
    Markdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Passed,
    Failed,
    Warning,
    Skipped,
    Unsupported,
    Error,
    Timeout,
}

impl TestStatus {
    pub fn label(self) -> &'static str {
        match self {
            TestStatus::Passed => "PASS",
            TestStatus::Failed => "FAIL",
            TestStatus::Warning => "WARN",
            TestStatus::Skipped => "SKIP",
            TestStatus::Unsupported => "UNSUPPORTED",
            TestStatus::Error => "ERROR",
            TestStatus::Timeout => "TIMEOUT",
        }
    }

    pub fn is_failure(self) -> bool {
        matches!(
            self,
            TestStatus::Failed | TestStatus::Error | TestStatus::Timeout
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub id: String,
    pub name: String,
    pub category: String,
    pub profile: String,
    pub status: TestStatus,
    pub score: u32,
    pub max_score: u32,
    pub latency_ms: u128,
    pub request_id: Option<String>,
    pub error: Option<String>,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub binary: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetInfo {
    pub base_url: String,
    pub model: String,
    pub embedding_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInfo {
    pub profile: String,
    pub started_at: String,
    pub ended_at: String,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreSummary {
    pub overall: u32,
    pub max: u32,
    pub grade: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileScore {
    pub score: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureStatus {
    Passed,
    Failed,
    Warning,
    Skipped,
    Unsupported,
}

impl From<TestStatus> for FeatureStatus {
    fn from(value: TestStatus) -> Self {
        match value {
            TestStatus::Passed => FeatureStatus::Passed,
            TestStatus::Failed | TestStatus::Error | TestStatus::Timeout => FeatureStatus::Failed,
            TestStatus::Warning => FeatureStatus::Warning,
            TestStatus::Skipped => FeatureStatus::Skipped,
            TestStatus::Unsupported => FeatureStatus::Unsupported,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub tool: ToolInfo,
    pub target: TargetInfo,
    pub run: RunInfo,
    pub score: ScoreSummary,
    pub profiles: BTreeMap<String, ProfileScore>,
    pub features: BTreeMap<String, FeatureStatus>,
    pub tests: Vec<TestResult>,
}

#[cfg(test)]
mod tests {
    use super::{FeatureStatus, Profile, TestStatus};

    #[test]
    fn profile_strings_and_display_are_stable() {
        let cases = [
            (Profile::Core, "core"),
            (Profile::Agent, "agent"),
            (Profile::Data, "data"),
            (Profile::Multimodal, "multimodal"),
            (Profile::Full, "full"),
            (Profile::Destructive, "destructive"),
        ];

        for (profile, expected) in cases {
            assert_eq!(profile.as_str(), expected);
            assert_eq!(profile.to_string(), expected);
        }
    }

    #[test]
    fn test_status_labels_and_failure_flags_are_stable() {
        let cases = [
            (TestStatus::Passed, "PASS", false),
            (TestStatus::Failed, "FAIL", true),
            (TestStatus::Warning, "WARN", false),
            (TestStatus::Skipped, "SKIP", false),
            (TestStatus::Unsupported, "UNSUPPORTED", false),
            (TestStatus::Error, "ERROR", true),
            (TestStatus::Timeout, "TIMEOUT", true),
        ];

        for (status, label, failure) in cases {
            assert_eq!(status.label(), label);
            assert_eq!(status.is_failure(), failure);
        }
    }

    #[test]
    fn feature_status_maps_from_test_status() {
        let cases = [
            (TestStatus::Passed, FeatureStatus::Passed),
            (TestStatus::Failed, FeatureStatus::Failed),
            (TestStatus::Warning, FeatureStatus::Warning),
            (TestStatus::Skipped, FeatureStatus::Skipped),
            (TestStatus::Unsupported, FeatureStatus::Unsupported),
            (TestStatus::Error, FeatureStatus::Failed),
            (TestStatus::Timeout, FeatureStatus::Failed),
        ];

        for (status, expected) in cases {
            assert_eq!(FeatureStatus::from(status), expected);
        }
    }
}
