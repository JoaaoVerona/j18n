use crate::model::{GeminiContent, GeminiPart, GenerateContentRequest, GenerateContentResponse, GenerationConfig};
use async_trait::async_trait;
use j18n_core::{J18nError, J18nResult, Language};
use j18n_translator::{create_extrapolated_values, restore_extrapolated_values, ExtrapolatedValue, I18nTranslator};
use reqwest::Client;
use std::time::Duration;

pub const GEMINI_API_KEY_ENV_VAR: &str = "GEMINI_API_KEY";

const DEFAULT_MODEL_NAME: &str = "gemini-3.1-pro-preview";
const SYSTEM_INSTRUCTIONS: &str =
	"Answer ONLY with a JSON array containing string elements, one for each translated value, \
	in the same order as their inputs. Do NOT embed the JSON array in Markdown, do NOT write \
	'```json' or equivalents; answer with a JSON array directly.";

pub struct GeminiApiI18nTranslator {
	api_key: String,
	client: Client,
	model_name: String,
	timeout: Duration,
}

impl GeminiApiI18nTranslator {
	pub const TRANSLATOR_ID: &'static str = "gemini-api";

	pub fn new() -> J18nResult<Self> {
		let api_key = std::env::var(GEMINI_API_KEY_ENV_VAR).map_err(|_| J18nError::EnvVarMissing {
			name: GEMINI_API_KEY_ENV_VAR,
		})?;
		let client = Client::builder()
			.timeout(Duration::from_secs(180))
			.build()
			.map_err(|e| J18nError::translator(format!("failed to build HTTP client: {e}")))?;

		Ok(Self {
			api_key,
			client,
			model_name: DEFAULT_MODEL_NAME.to_string(),
			timeout: Duration::from_secs(180),
		})
	}

	pub fn with_model_name(mut self, model_name: impl Into<String>) -> Self {
		self.model_name = model_name.into();
		self
	}

	pub fn with_timeout(mut self, timeout: Duration) -> Self {
		self.timeout = timeout;
		self
	}

	async fn translate_extrapolated_values(
		&self,
		extrapolated_values: &[ExtrapolatedValue],
		from: Language,
		to: Language,
	) -> J18nResult<Vec<String>> {
		let extrapolated_for_prompt: Vec<&str> = extrapolated_values
			.iter()
			.map(|v| v.extrapolated_value.as_str())
			.collect();
		let values_for_prompt_serialized = serde_json::to_string(&extrapolated_for_prompt)
			.map_err(|e| J18nError::translator(format!("failed to serialize prompt array: {e}")))?;
		let prompt = build_prompt(from, to);
		let response_text = self.complete_chat(vec![prompt, values_for_prompt_serialized]).await?;
		let parsed: Vec<String> = serde_json::from_str(response_text.trim()).map_err(|e| {
			J18nError::translator(format!(
				"Gemini did not return a JSON array of strings: {e}\nResponse:\n{response_text}"
			))
		})?;

		Ok(parsed)
	}

	async fn complete_chat(&self, messages: Vec<String>) -> J18nResult<String> {
		let contents: Vec<GeminiContent> = messages
			.into_iter()
			.map(|message| GeminiContent {
				parts: vec![GeminiPart { text: message }],
				role: Some("user".to_string()),
			})
			.collect();
		let request_body = GenerateContentRequest {
			contents,
			generation_config: Some(GenerationConfig { temperature: Some(1.0) }),
			system_instruction: Some(GeminiContent {
				parts: vec![GeminiPart {
					text: SYSTEM_INSTRUCTIONS.to_string(),
				}],
				role: None,
			}),
		};
		let url = format!(
			"https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
			self.model_name
		);
		let response = self
			.client
			.post(&url)
			.timeout(self.timeout)
			.header("content-type", "application/json")
			.header("x-goog-api-key", &self.api_key)
			.json(&request_body)
			.send()
			.await
			.map_err(|e| J18nError::translator(format!("Gemini request failed: {e}")))?;

		if !response.status().is_success() {
			let status = response.status();
			let body = response.text().await.unwrap_or_default();

			return Err(J18nError::translator(format!(
				"Gemini API returned HTTP {status}: {body}"
			)));
		}

		let parsed: GenerateContentResponse = response
			.json()
			.await
			.map_err(|e| J18nError::translator(format!("failed to parse Gemini response: {e}")))?;
		let first_candidate = parsed
			.candidates
			.into_iter()
			.next()
			.ok_or_else(|| J18nError::translator("no content candidate returned by Gemini API"))?;
		let joined_output: String = first_candidate
			.content
			.parts
			.into_iter()
			.map(|part| part.text)
			.collect::<Vec<_>>()
			.join("\n");

		Ok(joined_output)
	}
}

#[async_trait]
impl I18nTranslator for GeminiApiI18nTranslator {
	fn translator_id(&self) -> &str {
		Self::TRANSLATOR_ID
	}

	async fn translate_i18n_values(
		&self,
		from: Language,
		to: Language,
		values: Vec<String>,
	) -> J18nResult<Vec<String>> {
		let extrapolated_values = create_extrapolated_values(&values);
		let translated_values = self
			.translate_extrapolated_values(&extrapolated_values, from, to)
			.await?;

		restore_extrapolated_values(&extrapolated_values, &translated_values)
	}
}

fn build_prompt(from: Language, to: Language) -> String {
	[
		format!(
			"Translate the values in the following JSON array, from {} to {}.",
			from.language_name(),
			to.language_name()
		),
		"Consider that the context for the translation is a music streaming app.".to_string(),
		"DO NOT remove or modify HTML tags.".to_string(),
		"DO NOT remove, skip or modify placeholders, like [1], [2], [3], etc.".to_string(),
		"DO NOT translate the words 'artwork', 'feedback', 'playlist' and 'playlists'.".to_string(),
		"DO NOT translate the words 'touch', 'touch name', or anything else that might resemble a click or touch."
			.to_string(),
		"The word 'track' should be interpreted as 'song' when translating it.".to_string(),
		"Once again, DO NOT remove placeholders like '[1]', '[2]', '[3]', '[4]', etc.".to_string(),
		"Answer ONLY with a JSON array containing string elements, one for each translated value, in the same order as their inputs.".to_string(),
		"Do NOT embed the JSON array in Markdown, do NOT write '```json' or equivalents.".to_string(),
		"Answer with a JSON array directly.".to_string(),
		"The JSON array is:".to_string(),
	]
	.join("\n")
}
