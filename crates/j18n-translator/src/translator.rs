use async_trait::async_trait;
use j18n_core::J18nResult;

#[async_trait]
pub trait I18nTranslator: Send + Sync {
	fn translator_id(&self) -> &str;

	async fn translate_values(
		&self,
		from_language: &str,
		to_language: &str,
		values: Vec<String>,
	) -> J18nResult<Vec<String>>;
}
