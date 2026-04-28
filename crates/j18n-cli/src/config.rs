use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct I18nToolConfig {
	#[serde(rename = "baseDirectory")]
	pub base_directory: PathBuf,

	#[serde(rename = "generateI18nFor")]
	pub generate_i18n_for: Vec<String>,

	#[serde(rename = "referenceI18n")]
	pub reference_i18n: String,

	pub translator: TranslatorKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum TranslatorKind {
	ClaudeCode,
	GeminiApi,
}

pub fn load_config(path: &Path) -> Result<I18nToolConfig> {
	let raw = std::fs::read(path).with_context(|| format!("failed to read config file \"{}\"", path.display()))?;
	let config: I18nToolConfig =
		serde_json::from_slice(&raw).with_context(|| format!("failed to parse config file \"{}\"", path.display()))?;

	Ok(config)
}
