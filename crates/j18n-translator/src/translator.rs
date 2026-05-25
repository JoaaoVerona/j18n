use async_trait::async_trait;
use j18n_core::{ContentFormat, J18nResult};

#[async_trait]
pub trait I18nTranslator: Send + Sync {
	fn translator_id(&self) -> &str;

	/// Translates a batch of values from one language to another. `format`
	/// tells the implementation what kind of content the values are so it can
	/// frame the prompt appropriately — discrete UI strings ([`ContentFormat::Json`])
	/// versus whole Markdown/MDX documents ([`ContentFormat::Markdown`]), whose
	/// syntax must be preserved.
	async fn translate_values(
		&self,
		from_language: &str,
		to_language: &str,
		values: Vec<String>,
		format: ContentFormat,
	) -> J18nResult<Vec<String>>;
}
