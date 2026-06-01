use std::path::PathBuf;

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};

use crate::types::{Profile, ReportFormat};

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub name: Option<String>,
    pub base_url: String,
    pub api_key: Option<String>,
    pub api_key_env: String,
    pub no_auth: bool,
    pub model: String,
    pub embedding_model: Option<String>,
    pub profiles: Vec<Profile>,
    pub timeouts: TimeoutConfig,
    pub features: FeatureConfig,
    pub thresholds: ThresholdConfig,
    pub output: OutputConfig,
    pub concurrency: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    pub connect_ms: u64,
    pub request_ms: u64,
    pub stream_ms: u64,
    pub first_token_ms: u64,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            connect_ms: 5_000,
            request_ms: 60_000,
            stream_ms: 120_000,
            first_token_ms: 15_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureConfig {
    pub costly: bool,
    pub destructive: bool,
    pub strict: bool,
    pub redact: bool,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            costly: false,
            destructive: false,
            strict: false,
            redact: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThresholdConfig {
    pub min_score: Option<u32>,
    pub max_latency_ms: Option<u64>,
    pub max_first_token_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub format: ReportFormat,
    pub path: Option<PathBuf>,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: ReportFormat::Terminal,
            path: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunConfigInput {
    pub config_path: Option<PathBuf>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub model: Option<String>,
    pub embedding_model: Option<String>,
    pub profiles: Vec<Profile>,
    pub output_path: Option<PathBuf>,
    pub output_format: Option<ReportFormat>,
    pub timeout_ms: Option<u64>,
    pub stream_timeout_ms: Option<u64>,
    pub no_auth: bool,
    pub costly: bool,
    pub destructive: bool,
    pub strict: bool,
    pub redact: Option<bool>,
    pub min_score: Option<u32>,
    pub max_latency_ms: Option<u64>,
    pub max_first_token_ms: Option<u64>,
    pub concurrency: Option<usize>,
}

impl RunConfig {
    pub fn load(
        input: RunConfigInput,
        default_profile: Profile,
        require_chat_model: bool,
    ) -> anyhow::Result<Self> {
        let file = if let Some(path) = &input.config_path {
            Some(FileConfig::load(path)?)
        } else {
            None
        };

        let api_key_env = input
            .api_key_env
            .or_else(|| file.as_ref().and_then(|c| c.api_key_env.clone()))
            .unwrap_or_else(|| "OPENAI_API_KEY".to_string());

        let no_auth = input.no_auth || file.as_ref().and_then(|c| c.no_auth).unwrap_or(false);
        let api_key = if no_auth {
            None
        } else {
            input.api_key.or_else(|| std::env::var(&api_key_env).ok())
        };

        let base_url = input
            .base_url
            .or_else(|| file.as_ref().and_then(|c| c.base_url.clone()))
            .map(|s| s.trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());

        let Some(base_url) = base_url else {
            bail!("--base-url or config base_url is required");
        };

        let model = input
            .model
            .or_else(|| file.as_ref().and_then(|c| c.models.as_ref()?.chat.clone()))
            .unwrap_or_else(|| "unknown".to_string());

        if require_chat_model && model == "unknown" {
            bail!("--model or config models.chat is required");
        }

        let embedding_model = input.embedding_model.or_else(|| {
            file.as_ref()
                .and_then(|c| c.models.as_ref()?.embeddings.clone())
        });

        let profiles = if !input.profiles.is_empty() {
            input.profiles
        } else if let Some(profiles) = file.as_ref().and_then(|c| c.profiles.clone()) {
            if profiles.is_empty() {
                vec![default_profile]
            } else {
                profiles
            }
        } else {
            vec![default_profile]
        };

        let mut timeouts: TimeoutConfig = file
            .as_ref()
            .and_then(|c| c.timeouts.clone())
            .map(Into::into)
            .unwrap_or_default();
        if let Some(timeout_ms) = input.timeout_ms {
            timeouts.request_ms = timeout_ms;
        }
        if let Some(stream_timeout_ms) = input.stream_timeout_ms {
            timeouts.stream_ms = stream_timeout_ms;
        }

        let mut features: FeatureConfig = file
            .as_ref()
            .and_then(|c| c.features.clone())
            .map(Into::into)
            .unwrap_or_default();
        features.costly = input.costly || features.costly;
        features.destructive = input.destructive || features.destructive;
        features.strict = input.strict || features.strict;
        if let Some(redact) = input.redact {
            features.redact = redact;
        }

        let mut thresholds: ThresholdConfig = file
            .as_ref()
            .and_then(|c| c.thresholds.clone())
            .map(Into::into)
            .unwrap_or_default();
        thresholds.min_score = input.min_score.or(thresholds.min_score);
        thresholds.max_latency_ms = input.max_latency_ms.or(thresholds.max_latency_ms);
        thresholds.max_first_token_ms = input.max_first_token_ms.or(thresholds.max_first_token_ms);

        let mut output: OutputConfig = file
            .as_ref()
            .and_then(|c| c.output.clone())
            .map(Into::into)
            .unwrap_or_default();
        if let Some(format) = input.output_format {
            output.format = format;
        }
        output.path = input.output_path.or(output.path);

        Ok(Self {
            name: file.as_ref().and_then(|c| c.name.clone()),
            base_url,
            api_key,
            api_key_env,
            no_auth,
            model,
            embedding_model,
            profiles,
            timeouts,
            features,
            thresholds,
            output,
            concurrency: input.concurrency.unwrap_or(4).max(1),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
struct FileConfig {
    name: Option<String>,
    base_url: Option<String>,
    api_key_env: Option<String>,
    no_auth: Option<bool>,
    models: Option<FileModels>,
    profiles: Option<Vec<Profile>>,
    timeouts: Option<FileTimeouts>,
    features: Option<FileFeatures>,
    thresholds: Option<FileThresholds>,
    output: Option<FileOutput>,
}

impl FileConfig {
    fn load(path: &PathBuf) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        serde_yaml::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))
    }
}

#[derive(Debug, Clone, Deserialize)]
struct FileModels {
    chat: Option<String>,
    embeddings: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct FileTimeouts {
    connect_ms: Option<u64>,
    request_ms: Option<u64>,
    stream_ms: Option<u64>,
    first_token_ms: Option<u64>,
}

impl From<FileTimeouts> for TimeoutConfig {
    fn from(value: FileTimeouts) -> Self {
        let defaults = TimeoutConfig::default();
        Self {
            connect_ms: value.connect_ms.unwrap_or(defaults.connect_ms),
            request_ms: value.request_ms.unwrap_or(defaults.request_ms),
            stream_ms: value.stream_ms.unwrap_or(defaults.stream_ms),
            first_token_ms: value.first_token_ms.unwrap_or(defaults.first_token_ms),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct FileFeatures {
    costly: Option<bool>,
    destructive: Option<bool>,
    strict: Option<bool>,
    redact: Option<bool>,
}

impl From<FileFeatures> for FeatureConfig {
    fn from(value: FileFeatures) -> Self {
        let defaults = FeatureConfig::default();
        Self {
            costly: value.costly.unwrap_or(defaults.costly),
            destructive: value.destructive.unwrap_or(defaults.destructive),
            strict: value.strict.unwrap_or(defaults.strict),
            redact: value.redact.unwrap_or(defaults.redact),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct FileThresholds {
    min_score: Option<u32>,
    max_latency_ms: Option<u64>,
    max_first_token_ms: Option<u64>,
}

impl From<FileThresholds> for ThresholdConfig {
    fn from(value: FileThresholds) -> Self {
        Self {
            min_score: value.min_score,
            max_latency_ms: value.max_latency_ms,
            max_first_token_ms: value.max_first_token_ms,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct FileOutput {
    format: Option<ReportFormat>,
    path: Option<PathBuf>,
}

impl From<FileOutput> for OutputConfig {
    fn from(value: FileOutput) -> Self {
        let defaults = OutputConfig::default();
        Self {
            format: value.format.unwrap_or(defaults.format),
            path: value.path,
        }
    }
}

pub fn template_config() -> &'static str {
    r#"name: local-provider
base_url: http://localhost:8000/v1
api_key_env: OPENAI_API_KEY
no_auth: true

models:
  chat: mock-chat
  embeddings: mock-embedding

profiles:
  - core

timeouts:
  connect_ms: 5000
  request_ms: 60000
  stream_ms: 120000
  first_token_ms: 15000

features:
  costly: false
  destructive: false
  strict: false
  redact: true

thresholds:
  min_score: 80
  max_latency_ms: 10000
  max_first_token_ms: 5000

output:
  format: json
  path: reports/provider.json
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_cli_only_defaults_and_trims_base_url() {
        let config = RunConfig::load(
            RunConfigInput {
                base_url: Some("http://localhost:8000/v1///".to_string()),
                model: Some("mock-chat".to_string()),
                no_auth: true,
                concurrency: Some(0),
                ..RunConfigInput::default()
            },
            Profile::Core,
            true,
        )
        .unwrap();

        assert_eq!(config.base_url, "http://localhost:8000/v1");
        assert_eq!(config.model, "mock-chat");
        assert_eq!(config.api_key_env, "OPENAI_API_KEY");
        assert!(config.no_auth);
        assert!(config.api_key.is_none());
        assert_eq!(config.profiles, vec![Profile::Core]);
        assert_eq!(config.timeouts.request_ms, 60_000);
        assert_eq!(config.timeouts.stream_ms, 120_000);
        assert_eq!(config.output.format, ReportFormat::Terminal);
        assert_eq!(config.concurrency, 1);
    }

    #[test]
    fn config_defaults_are_stable() {
        let features = FeatureConfig::default();
        assert!(!features.costly);
        assert!(!features.destructive);
        assert!(!features.strict);
        assert!(features.redact);

        let output = OutputConfig::default();
        assert_eq!(output.format, ReportFormat::Terminal);
        assert!(output.path.is_none());
    }

    #[test]
    fn merges_file_config_with_cli_overrides() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("provider.yaml");
        std::fs::write(
            &path,
            r#"
name: file-provider
base_url: http://from-file/v1
api_key_env: FILE_KEY
no_auth: false
models:
  chat: file-chat
  embeddings: file-embed
profiles:
  - agent
timeouts:
  request_ms: 111
  stream_ms: 222
features:
  strict: true
  redact: false
thresholds:
  min_score: 70
output:
  format: markdown
  path: file.md
"#,
        )
        .unwrap();

        let config = RunConfig::load(
            RunConfigInput {
                config_path: Some(path),
                base_url: Some("http://override/v1/".to_string()),
                api_key: Some("sk-test".to_string()),
                model: Some("cli-chat".to_string()),
                profiles: vec![Profile::Data],
                output_format: Some(ReportFormat::Json),
                timeout_ms: Some(333),
                redact: Some(true),
                min_score: Some(90),
                ..RunConfigInput::default()
            },
            Profile::Core,
            true,
        )
        .unwrap();

        assert_eq!(config.name.as_deref(), Some("file-provider"));
        assert_eq!(config.base_url, "http://override/v1");
        assert_eq!(config.api_key.as_deref(), Some("sk-test"));
        assert_eq!(config.api_key_env, "FILE_KEY");
        assert_eq!(config.model, "cli-chat");
        assert_eq!(config.embedding_model.as_deref(), Some("file-embed"));
        assert_eq!(config.profiles, vec![Profile::Data]);
        assert_eq!(config.timeouts.request_ms, 333);
        assert_eq!(config.timeouts.stream_ms, 222);
        assert!(config.features.strict);
        assert!(config.features.redact);
        assert_eq!(config.thresholds.min_score, Some(90));
        assert_eq!(config.output.format, ReportFormat::Json);
        assert_eq!(
            config.output.path.as_deref(),
            Some(std::path::Path::new("file.md"))
        );
    }

    #[test]
    fn no_auth_ignores_api_key() {
        let config = RunConfig::load(
            RunConfigInput {
                base_url: Some("http://localhost:8000/v1".to_string()),
                api_key: Some("sk-should-not-be-used".to_string()),
                model: Some("mock-chat".to_string()),
                no_auth: true,
                ..RunConfigInput::default()
            },
            Profile::Core,
            true,
        )
        .unwrap();

        assert!(config.no_auth);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn require_chat_model_is_enforced() {
        let err = RunConfig::load(
            RunConfigInput {
                base_url: Some("http://localhost:8000/v1".to_string()),
                ..RunConfigInput::default()
            },
            Profile::Core,
            true,
        )
        .unwrap_err();

        assert!(err.to_string().contains("--model"));
    }

    #[test]
    fn base_url_is_required() {
        let err = RunConfig::load(
            RunConfigInput {
                model: Some("mock-chat".to_string()),
                ..RunConfigInput::default()
            },
            Profile::Core,
            true,
        )
        .unwrap_err();

        assert!(err.to_string().contains("--base-url"));
    }

    #[test]
    fn reads_api_key_from_environment_when_auth_enabled() {
        std::env::set_var("OCTEST_UNIT_API_KEY", "sk-env-test");
        let config = RunConfig::load(
            RunConfigInput {
                base_url: Some("http://localhost:8000/v1".to_string()),
                api_key_env: Some("OCTEST_UNIT_API_KEY".to_string()),
                model: Some("mock-chat".to_string()),
                ..RunConfigInput::default()
            },
            Profile::Core,
            true,
        )
        .unwrap();
        std::env::remove_var("OCTEST_UNIT_API_KEY");

        assert_eq!(config.api_key.as_deref(), Some("sk-env-test"));
    }

    #[test]
    fn allows_embedding_only_config_without_chat_model() {
        let config = RunConfig::load(
            RunConfigInput {
                base_url: Some("http://localhost:8000/v1".to_string()),
                embedding_model: Some("mock-embedding".to_string()),
                no_auth: true,
                ..RunConfigInput::default()
            },
            Profile::Data,
            false,
        )
        .unwrap();

        assert_eq!(config.model, "unknown");
        assert_eq!(config.embedding_model.as_deref(), Some("mock-embedding"));
    }

    #[test]
    fn reports_config_parse_error() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bad.yaml");
        std::fs::write(&path, "profiles: [").unwrap();

        let err = RunConfig::load(
            RunConfigInput {
                config_path: Some(path),
                ..RunConfigInput::default()
            },
            Profile::Core,
            true,
        )
        .unwrap_err();

        assert!(err.to_string().contains("failed to parse config"));
    }

    #[test]
    fn empty_file_profiles_fall_back_to_default_profile_and_stream_timeout_overrides() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("provider.yaml");
        std::fs::write(
            &path,
            r#"
base_url: http://from-file/v1
models:
  chat: file-chat
profiles: []
"#,
        )
        .unwrap();

        let config = RunConfig::load(
            RunConfigInput {
                config_path: Some(path),
                api_key_env: Some("CLI_KEY_ENV".to_string()),
                stream_timeout_ms: Some(999),
                no_auth: true,
                ..RunConfigInput::default()
            },
            Profile::Agent,
            true,
        )
        .unwrap();

        assert_eq!(config.api_key_env, "CLI_KEY_ENV");
        assert_eq!(config.profiles, vec![Profile::Agent]);
        assert_eq!(config.timeouts.stream_ms, 999);
    }

    #[test]
    fn template_config_is_valid_yaml() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("provider.yaml");
        std::fs::write(&path, template_config()).unwrap();

        let config = RunConfig::load(
            RunConfigInput {
                config_path: Some(path),
                ..RunConfigInput::default()
            },
            Profile::Core,
            true,
        )
        .unwrap();

        assert_eq!(config.model, "mock-chat");
        assert_eq!(config.embedding_model.as_deref(), Some("mock-embedding"));
        assert!(config.no_auth);
    }
}
