use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct GeminiContent {
	pub parts: Vec<GeminiPart>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub role: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GeminiPart {
	pub text: String,
}

#[derive(Debug, Serialize)]
pub struct GenerationConfig {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub temperature: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct GenerateContentRequest {
	pub contents: Vec<GeminiContent>,
	#[serde(rename = "generation_config", skip_serializing_if = "Option::is_none")]
	pub generation_config: Option<GenerationConfig>,
	#[serde(rename = "system_instruction", skip_serializing_if = "Option::is_none")]
	pub system_instruction: Option<GeminiContent>,
}

#[derive(Debug, Deserialize)]
pub struct GenerateContentResponse {
	pub candidates: Vec<GenerateContentCandidate>,
}

#[derive(Debug, Deserialize)]
pub struct GenerateContentCandidate {
	pub content: GeminiContent,
}
