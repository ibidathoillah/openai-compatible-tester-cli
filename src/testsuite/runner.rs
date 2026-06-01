use std::time::Instant;

use chrono::Utc;

use crate::client::ApiClient;
use crate::config::RunConfig;
use crate::testsuite::registry::{BuiltinTest, TestCase};
use crate::testsuite::score::{calculate_score, feature_summary, profile_scores};
use crate::types::{CompatibilityReport, RunInfo, TargetInfo, TestResult, ToolInfo};

#[derive(Clone)]
pub struct Runner {
    config: RunConfig,
    client: ApiClient,
}

impl Runner {
    pub fn new(config: RunConfig, client: ApiClient) -> Self {
        Self { config, client }
    }

    pub async fn run(&self, tests: Vec<BuiltinTest>) -> RunOutput {
        let started_at = Utc::now();
        let started = Instant::now();
        let mut results = Vec::with_capacity(tests.len());

        for test in tests {
            tracing::debug!(
                id = test.id(),
                name = test.name(),
                category = test.category(),
                weight = test.weight(),
                profiles = ?test.profiles(),
                required = test.required(),
                "running compatibility test"
            );
            let result = test.run(&self.config, &self.client).await;
            results.push(result);
        }

        RunOutput {
            started_at: started_at.to_rfc3339(),
            ended_at: Utc::now().to_rfc3339(),
            duration_ms: started.elapsed().as_millis(),
            results,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunOutput {
    pub started_at: String,
    pub ended_at: String,
    pub duration_ms: u128,
    pub results: Vec<TestResult>,
}

pub fn build_report(config: &RunConfig, output: RunOutput) -> CompatibilityReport {
    let profile_name = config
        .profiles
        .iter()
        .map(|profile| profile.as_str())
        .collect::<Vec<_>>()
        .join(",");

    let score = calculate_score(&output.results);
    let profiles = profile_scores(&output.results);
    let features = feature_summary(&output.results);

    CompatibilityReport {
        tool: ToolInfo {
            name: "openai-compatible-tester-cli".to_string(),
            binary: "octest".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        target: TargetInfo {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            embedding_model: config.embedding_model.clone(),
        },
        run: RunInfo {
            profile: profile_name,
            started_at: output.started_at,
            ended_at: output.ended_at,
            duration_ms: output.duration_ms,
        },
        score,
        profiles,
        features,
        tests: output.results,
    }
}
