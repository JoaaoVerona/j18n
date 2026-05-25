use serde::Deserialize;
use std::fmt;

/// The kind of file a reference/target pair represents, which determines how a
/// file is parsed into translatable entries and serialized back.
///
/// - [`ContentFormat::Json`] flattens an i18n JSON object into many dotted-key
///   entries (the original, default behavior).
/// - [`ContentFormat::Markdown`] treats the entire file as a single entry whose
///   value is the whole document body — used for translating Markdown / MDX
///   documents (e.g. Docusaurus docs) while preserving their syntax.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentFormat {
	#[default]
	Json,
	Markdown,
}

/// The single, format-stable entry key used for every [`ContentFormat::Markdown`]
/// file. A Markdown file maps to exactly one entry, so the key only has to be
/// constant across the reference and its targets (so sync pairs them) and a
/// valid hash-cache key (non-empty, no `=` or newlines).
pub const MARKDOWN_ENTRY_KEY: &str = "content";

impl ContentFormat {
	pub fn is_markdown(self) -> bool {
		matches!(self, Self::Markdown)
	}
}

impl fmt::Display for ContentFormat {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Json => f.write_str("JSON"),
			Self::Markdown => f.write_str("MARKDOWN"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_is_json() {
		assert_eq!(ContentFormat::default(), ContentFormat::Json);
	}

	#[test]
	fn display_uses_uppercase_constant_form() {
		assert_eq!(ContentFormat::Json.to_string(), "JSON");
		assert_eq!(ContentFormat::Markdown.to_string(), "MARKDOWN");
	}

	#[test]
	fn is_markdown_only_true_for_markdown() {
		assert!(ContentFormat::Markdown.is_markdown());
		assert!(!ContentFormat::Json.is_markdown());
	}

	#[test]
	fn deserializes_from_lowercase_strings() {
		assert_eq!(
			serde_json::from_str::<ContentFormat>("\"json\"").unwrap(),
			ContentFormat::Json
		);
		assert_eq!(
			serde_json::from_str::<ContentFormat>("\"markdown\"").unwrap(),
			ContentFormat::Markdown
		);
	}

	#[test]
	fn rejects_unknown_format_string() {
		assert!(serde_json::from_str::<ContentFormat>("\"yaml\"").is_err());
	}
}
