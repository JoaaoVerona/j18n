use async_trait::async_trait;
use j18n_core::{J18nError, J18nResult, Language};
use j18n_translator::{create_extrapolated_values, restore_extrapolated_values, ExtrapolatedValue, I18nTranslator};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const ENTRY_SEPARATOR: &str = "<<<SEP>>>";

pub struct ClaudeCodeBasedI18nTranslator;

impl ClaudeCodeBasedI18nTranslator {
	pub const TRANSLATOR_ID: &'static str = "claude-code";

	pub fn new() -> Self {
		Self
	}
}

impl Default for ClaudeCodeBasedI18nTranslator {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl I18nTranslator for ClaudeCodeBasedI18nTranslator {
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
		let translated_values = translate_extrapolated_values(&extrapolated_values, from, to).await?;

		restore_extrapolated_values(&extrapolated_values, &translated_values)
	}
}

async fn translate_extrapolated_values(
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
	let prompt = build_prompt(from, to, &values_for_prompt_serialized);
	let response = execute_claude_code(&prompt).await?;

	Ok(response
		.split(ENTRY_SEPARATOR)
		.map(|s| s.trim().to_string())
		.filter(|s| !s.is_empty())
		.collect())
}

fn build_prompt(from: Language, to: Language, values_for_prompt_serialized: &str) -> String {
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
		format!(
			"Answer ONLY with the translated values, one per line, each separated by the exact string '{ENTRY_SEPARATOR}' on its own line, in the same order as the inputs."
		),
		format!(
			"Do NOT include any other text, explanations, numbering, or formatting — only the translated values separated by '{ENTRY_SEPARATOR}'."
		),
		"The JSON array of values to translate is:".to_string(),
		values_for_prompt_serialized.to_string(),
	]
	.join("\n")
}

async fn execute_claude_code(prompt: &str) -> J18nResult<String> {
	let mut command = if cfg!(target_os = "windows") {
		let mut command = Command::new("cmd");

		command.args(["/C", "claude", "--model=opus", "-p"]);
		command
	} else {
		let mut command = Command::new("claude");

		command.args(["--model=opus", "-p"]);
		command
	};

	command
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());

	let mut child = command
		.spawn()
		.map_err(|e| J18nError::translator(format!("failed to spawn Claude Code process: {e}")))?;

	if let Some(stdin) = child.stdin.as_mut() {
		stdin
			.write_all(prompt.as_bytes())
			.await
			.map_err(|e| J18nError::translator(format!("failed to write prompt to Claude Code: {e}")))?;
		stdin
			.shutdown()
			.await
			.map_err(|e| J18nError::translator(format!("failed to close Claude Code stdin: {e}")))?;
	}

	let output = child
		.wait_with_output()
		.await
		.map_err(|e| J18nError::translator(format!("failed to wait for Claude Code: {e}")))?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
		let exit_code = output
			.status
			.code()
			.map(|c| c.to_string())
			.unwrap_or_else(|| "<signal>".to_string());

		return Err(J18nError::translator(format!(
			"Claude Code process exited with code {exit_code}. Stderr: {stderr}"
		)));
	}

	Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
