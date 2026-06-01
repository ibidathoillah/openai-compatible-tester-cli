use std::collections::BTreeMap;

use crate::types::{FeatureStatus, ProfileScore, ScoreSummary, TestResult, TestStatus};

pub fn calculate_score(results: &[TestResult]) -> ScoreSummary {
    let max: u32 = results.iter().map(|result| result.max_score).sum();
    let score: u32 = results.iter().map(|result| result.score).sum();
    let overall = if max == 0 {
        0
    } else {
        ((score as f64 / max as f64) * 100.0).round() as u32
    };

    ScoreSummary {
        overall,
        max: 100,
        grade: grade(overall).to_string(),
    }
}

pub fn profile_scores(results: &[TestResult]) -> BTreeMap<String, ProfileScore> {
    let mut grouped: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for result in results {
        let entry = grouped.entry(result.profile.clone()).or_default();
        entry.0 += result.score;
        entry.1 += result.max_score;
    }

    grouped
        .into_iter()
        .map(|(profile, (score, max))| {
            let normalized = if max == 0 {
                0
            } else {
                ((score as f64 / max as f64) * 100.0).round() as u32
            };
            (
                profile,
                ProfileScore {
                    score: normalized,
                    max: 100,
                },
            )
        })
        .collect()
}

pub fn feature_summary(results: &[TestResult]) -> BTreeMap<String, FeatureStatus> {
    let mut grouped: BTreeMap<String, Vec<TestStatus>> = BTreeMap::new();
    for result in results {
        grouped
            .entry(feature_name(&result.category).to_string())
            .or_default()
            .push(result.status);
    }

    grouped
        .into_iter()
        .map(|(feature, statuses)| (feature, summarize_statuses(&statuses)))
        .collect()
}

fn summarize_statuses(statuses: &[TestStatus]) -> FeatureStatus {
    if statuses.iter().any(|status| status.is_failure()) {
        FeatureStatus::Failed
    } else if statuses
        .iter()
        .any(|status| matches!(status, TestStatus::Warning))
    {
        FeatureStatus::Warning
    } else if statuses
        .iter()
        .all(|status| matches!(status, TestStatus::Unsupported))
    {
        FeatureStatus::Unsupported
    } else if statuses
        .iter()
        .all(|status| matches!(status, TestStatus::Skipped))
    {
        FeatureStatus::Skipped
    } else {
        FeatureStatus::Passed
    }
}

fn feature_name(category: &str) -> &str {
    match category {
        "auth" => "authentication",
        "models" => "models",
        "chat" => "chat_completions",
        "streaming" => "streaming",
        "tools" => "tool_calling",
        "schema" => "structured_outputs",
        "embeddings" => "embeddings",
        "errors" => "error_compatibility",
        "usage" => "reported_usage",
        other => other,
    }
}

fn grade(score: u32) -> &'static str {
    match score {
        95..=100 => "full_compatible",
        85..=94 => "production_compatible",
        70..=84 => "agent_core_compatible",
        50..=69 => "partial_compatible",
        25..=49 => "minimal_compatible",
        _ => "not_compatible",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{calculate_score, feature_summary, profile_scores};
    use crate::types::{FeatureStatus, TestResult, TestStatus};

    #[test]
    fn calculates_normalized_score() {
        let results = vec![
            test("a", TestStatus::Passed, 10, 10),
            test("b", TestStatus::Warning, 5, 10),
        ];
        assert_eq!(calculate_score(&results).overall, 75);
    }

    #[test]
    fn empty_results_score_zero() {
        let score = calculate_score(&[]);

        assert_eq!(score.overall, 0);
        assert_eq!(score.max, 100);
        assert_eq!(score.grade, "not_compatible");
    }

    #[test]
    fn assigns_grade_boundaries() {
        assert_eq!(
            calculate_score(&[test("a", TestStatus::Passed, 95, 100)]).grade,
            "full_compatible"
        );
        assert_eq!(
            calculate_score(&[test("a", TestStatus::Passed, 85, 100)]).grade,
            "production_compatible"
        );
        assert_eq!(
            calculate_score(&[test("a", TestStatus::Passed, 70, 100)]).grade,
            "agent_core_compatible"
        );
        assert_eq!(
            calculate_score(&[test("a", TestStatus::Passed, 50, 100)]).grade,
            "partial_compatible"
        );
        assert_eq!(
            calculate_score(&[test("a", TestStatus::Passed, 25, 100)]).grade,
            "minimal_compatible"
        );
        assert_eq!(
            calculate_score(&[test("a", TestStatus::Passed, 24, 100)]).grade,
            "not_compatible"
        );
    }

    #[test]
    fn calculates_profile_scores_independently() {
        let mut core = test("core", TestStatus::Passed, 10, 10);
        core.profile = "core".to_string();
        let mut agent = test("agent", TestStatus::Warning, 5, 10);
        agent.profile = "agent".to_string();

        let scores = profile_scores(&[core, agent]);

        assert_eq!(scores["core"].score, 100);
        assert_eq!(scores["agent"].score, 50);
    }

    #[test]
    fn profile_score_with_zero_max_is_zero() {
        let mut result = test("zero", TestStatus::Skipped, 0, 0);
        result.profile = "optional".to_string();

        let scores = profile_scores(&[result]);

        assert_eq!(scores["optional"].score, 0);
        assert_eq!(scores["optional"].max, 100);
    }

    #[test]
    fn feature_summary_uses_failure_precedence_and_category_mapping() {
        let mut passed_chat = test("chat.basic", TestStatus::Passed, 10, 10);
        passed_chat.category = "chat".to_string();
        let mut failed_chat = test("chat.stream", TestStatus::Failed, 0, 10);
        failed_chat.category = "chat".to_string();
        let mut warning_usage = test("usage", TestStatus::Warning, 2, 5);
        warning_usage.category = "usage".to_string();

        let features = feature_summary(&[passed_chat, failed_chat, warning_usage]);

        assert_eq!(features["chat_completions"], FeatureStatus::Failed);
        assert_eq!(features["reported_usage"], FeatureStatus::Warning);
    }

    #[test]
    fn feature_summary_handles_unsupported_skipped_and_unknown_categories() {
        let mut unsupported = test("x", TestStatus::Unsupported, 0, 0);
        unsupported.category = "embeddings".to_string();
        let mut skipped = test("y", TestStatus::Skipped, 0, 0);
        skipped.category = "optional".to_string();
        let mut passed = test("z", TestStatus::Passed, 1, 1);
        passed.category = "custom".to_string();

        let features = feature_summary(&[unsupported, skipped, passed]);

        assert_eq!(features["embeddings"], FeatureStatus::Unsupported);
        assert_eq!(features["optional"], FeatureStatus::Skipped);
        assert_eq!(features["custom"], FeatureStatus::Passed);
    }

    fn test(id: &str, status: TestStatus, score: u32, max_score: u32) -> TestResult {
        TestResult {
            id: id.to_string(),
            name: id.to_string(),
            category: "core".to_string(),
            profile: "core".to_string(),
            status,
            score,
            max_score,
            latency_ms: 0,
            request_id: None,
            error: None,
            details: json!({}),
        }
    }
}
