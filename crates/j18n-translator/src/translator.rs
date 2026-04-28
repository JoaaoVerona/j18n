use async_trait::async_trait;
use j18n_core::{J18nResult, Language};

#[async_trait]
pub trait I18nTranslator: Send + Sync {
	fn translator_id(&self) -> &str;

	async fn translate_i18n_values(&self, from: Language, to: Language, values: Vec<String>)
		-> J18nResult<Vec<String>>;
}
