use anyhow::Context;
use comfy_table::{Cell, Table};

use crate::types::{CompatibilityReport, ReportFormat, TestStatus};

#[derive(Debug, Clone, Copy)]
pub enum ReporterKind {
    Terminal,
    Json,
    Markdown,
}

impl From<ReportFormat> for ReporterKind {
    fn from(value: ReportFormat) -> Self {
        match value {
            ReportFormat::Terminal => ReporterKind::Terminal,
            ReportFormat::Json => ReporterKind::Json,
            ReportFormat::Markdown => ReporterKind::Markdown,
        }
    }
}

pub fn render_report(kind: ReporterKind, report: &CompatibilityReport) -> anyhow::Result<String> {
    match kind {
        ReporterKind::Terminal => Ok(render_terminal(report)),
        ReporterKind::Json => {
            serde_json::to_string_pretty(report).context("failed to render JSON report")
        }
        ReporterKind::Markdown => Ok(render_markdown(report)),
    }
}

fn render_terminal(report: &CompatibilityReport) -> String {
    let mut out = String::new();
    out.push_str("OpenAI Compatible Tester\n\n");
    out.push_str(&format!("Provider : {}\n", report.target.base_url));
    out.push_str(&format!("Model    : {}\n", report.target.model));
    if let Some(model) = &report.target.embedding_model {
        out.push_str(&format!("Embedding: {model}\n"));
    }
    out.push_str(&format!("Profile  : {}\n", report.run.profile));
    out.push_str(&format!("Started  : {}\n\n", report.run.started_at));

    let mut table = Table::new();
    table.set_header(vec!["Status", "Test", "Category", "Latency", "Message"]);
    for test in &report.tests {
        table.add_row(vec![
            Cell::new(test.status.label()),
            Cell::new(&test.id),
            Cell::new(&test.category),
            Cell::new(format!("{}ms", test.latency_ms)),
            Cell::new(test.error.clone().unwrap_or_default()),
        ]);
    }
    out.push_str(&table.to_string());
    out.push_str("\n\nScore\n");
    for (profile, score) in &report.profiles {
        out.push_str(&format!(
            "  {:<10}: {}/{}\n",
            profile, score.score, score.max
        ));
    }
    out.push_str(&format!(
        "  {:<10}: {}/{}\n",
        "overall", report.score.overall, report.score.max
    ));
    out.push_str(&format!("\nGrade: {}\n", report.score.grade));
    out
}

fn render_markdown(report: &CompatibilityReport) -> String {
    let mut out = String::new();
    out.push_str("# OpenAI Compatibility Report\n\n");
    out.push_str(&format!("Provider: `{}`  \n", report.target.base_url));
    out.push_str(&format!("Model: `{}`  \n", report.target.model));
    if let Some(model) = &report.target.embedding_model {
        out.push_str(&format!("Embedding model: `{model}`  \n"));
    }
    out.push_str(&format!(
        "Score: `{}/{}`  \n",
        report.score.overall, report.score.max
    ));
    out.push_str(&format!("Grade: `{}`\n\n", report.score.grade));

    out.push_str("## Summary\n\n");
    out.push_str("| Feature | Status |\n|---|---|\n");
    for (feature, status) in &report.features {
        out.push_str(&format!("| {} | {:?} |\n", feature, status));
    }

    out.push_str("\n## Tests\n\n");
    out.push_str("| Test | Category | Status | Score | Latency | Message |\n");
    out.push_str("|---|---|---:|---:|---:|---|\n");
    for test in &report.tests {
        out.push_str(&format!(
            "| `{}` | {} | {} | {}/{} | {}ms | {} |\n",
            test.id,
            test.category,
            markdown_status(test.status),
            test.score,
            test.max_score,
            test.latency_ms,
            escape_markdown_table(test.error.as_deref().unwrap_or(""))
        ));
    }
    out
}

fn markdown_status(status: TestStatus) -> &'static str {
    match status {
        TestStatus::Passed => "Passed",
        TestStatus::Failed => "Failed",
        TestStatus::Warning => "Warning",
        TestStatus::Skipped => "Skipped",
        TestStatus::Unsupported => "Unsupported",
        TestStatus::Error => "Error",
        TestStatus::Timeout => "Timeout",
    }
}

fn escape_markdown_table(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{render_report, ReporterKind};
    use crate::types::{
        CompatibilityReport, FeatureStatus, ProfileScore, RunInfo, ScoreSummary, TargetInfo,
        TestResult, TestStatus, ToolInfo,
    };

    #[test]
    fn renders_json_report() {
        let rendered = render_report(ReporterKind::Json, &sample_report()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        assert_eq!(value["tool"]["binary"], "octest");
        assert_eq!(value["score"]["overall"], 82);
        assert_eq!(value["tests"][0]["id"], "chat.basic");
    }

    #[test]
    fn renders_markdown_and_escapes_table_cells() {
        let rendered = render_report(ReporterKind::Markdown, &sample_report()).unwrap();

        assert!(rendered.contains("# OpenAI Compatibility Report"));
        assert!(rendered.contains("| chat_completions | Passed |"));
        assert!(rendered.contains("bad\\|pipe next line"));
    }

    #[test]
    fn renders_terminal_summary() {
        let rendered = render_report(ReporterKind::Terminal, &sample_report()).unwrap();

        assert!(rendered.contains("OpenAI Compatible Tester"));
        assert!(rendered.contains("Provider : http://localhost:8080/v1"));
        assert!(rendered.contains("Grade: production_compatible"));
    }

    #[test]
    fn reporter_kind_maps_all_report_formats() {
        assert!(matches!(
            ReporterKind::from(crate::types::ReportFormat::Terminal),
            ReporterKind::Terminal
        ));
        assert!(matches!(
            ReporterKind::from(crate::types::ReportFormat::Json),
            ReporterKind::Json
        ));
        assert!(matches!(
            ReporterKind::from(crate::types::ReportFormat::Markdown),
            ReporterKind::Markdown
        ));
    }

    #[test]
    fn markdown_status_renders_all_statuses() {
        let cases = [
            (TestStatus::Passed, "Passed"),
            (TestStatus::Failed, "Failed"),
            (TestStatus::Warning, "Warning"),
            (TestStatus::Skipped, "Skipped"),
            (TestStatus::Unsupported, "Unsupported"),
            (TestStatus::Error, "Error"),
            (TestStatus::Timeout, "Timeout"),
        ];

        for (status, expected) in cases {
            assert_eq!(super::markdown_status(status), expected);
        }
    }

    fn sample_report() -> CompatibilityReport {
        let mut profiles = BTreeMap::new();
        profiles.insert(
            "core".to_string(),
            ProfileScore {
                score: 82,
                max: 100,
            },
        );

        let mut features = BTreeMap::new();
        features.insert("chat_completions".to_string(), FeatureStatus::Passed);

        CompatibilityReport {
            tool: ToolInfo {
                name: "openai-compatible-tester-cli".to_string(),
                binary: "octest".to_string(),
                version: "0.1.0".to_string(),
            },
            target: TargetInfo {
                base_url: "http://localhost:8080/v1".to_string(),
                model: "mock-chat".to_string(),
                embedding_model: Some("mock-embedding".to_string()),
            },
            run: RunInfo {
                profile: "core".to_string(),
                started_at: "2026-06-02T00:00:00Z".to_string(),
                ended_at: "2026-06-02T00:00:01Z".to_string(),
                duration_ms: 1_000,
            },
            score: ScoreSummary {
                overall: 82,
                max: 100,
                grade: "production_compatible".to_string(),
            },
            profiles,
            features,
            tests: vec![TestResult {
                id: "chat.basic".to_string(),
                name: "Basic chat completion".to_string(),
                category: "chat".to_string(),
                profile: "core".to_string(),
                status: TestStatus::Passed,
                score: 10,
                max_score: 10,
                latency_ms: 123,
                request_id: Some("req-1".to_string()),
                error: Some("bad|pipe\nnext line".to_string()),
                details: json!({"content_length": 4}),
            }],
        }
    }
}
