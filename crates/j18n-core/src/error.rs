use std::path::PathBuf;
use thiserror::Error;

pub type J18nResult<T> = Result<T, J18nError>;

#[derive(Debug, Error)]
pub enum J18nError {
	#[error("environment variable {name} is not set")]
	EnvVarMissing { name: &'static str },

	#[error("I/O error reading or writing {path}: {source}")]
	Io {
		path: PathBuf,
		#[source]
		source: std::io::Error,
	},

	#[error("invalid JSON in {path}: {source}")]
	Json {
		path: PathBuf,
		#[source]
		source: serde_json::Error,
	},

	#[error("invalid path pattern \"{pattern}\": {reason}")]
	InvalidPattern { pattern: String, reason: String },

	#[error("invalid regex pattern \"{pattern}\": {reason}")]
	InvalidRegex { pattern: String, reason: String },

	#[error("missing translation for \"{key}\" in generated JSON")]
	MissingTranslation { key: String },

	#[error("translator error: {0}")]
	Translator(String),

	#[error("validation error: {0}")]
	Validation(String),

	#[error("{0}")]
	Other(String),
}

impl J18nError {
	pub fn translator(message: impl Into<String>) -> Self {
		Self::Translator(message.into())
	}

	pub fn validation(message: impl Into<String>) -> Self {
		Self::Validation(message.into())
	}
}
