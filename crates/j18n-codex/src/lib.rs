use async_trait::async_trait;
use j18n_core::{ContentFormat, J18nError, J18nResult};
use j18n_translator::I18nTranslator;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const ENTRY_SEPARATOR: &str = "<<<SEP>>>";

#[async_trait]
pub trait CodexExecutor: Send + Sync {
	async fn execute(&self, prompt: &str) -> J18nResult<String>;
}

pub struct DefaultCodexExecutor {
	model: String,
	effort: String,
}

impl DefaultCodexExecutor {
	pub fn new(model: impl Into<String>, effort: impl Into<String>) -> Self {
		Self {
			model: model.into(),
			effort: effort.into(),
		}
	}
}

#[async_trait]
impl CodexExecutor for DefaultCodexExecutor {
	async fn execute(&self, prompt: &str) -> J18nResult<String> {
		execute_codex(&self.model, &self.effort, prompt).await
	}
}

pub struct CodexCliBasedI18nTranslator<E: CodexExecutor = DefaultCodexExecutor> {
	additional_prompts: Vec<String>,
	effort: String,
	executor: E,
}

impl CodexCliBasedI18nTranslator<DefaultCodexExecutor> {
	pub const TRANSLATOR_ID: &'static str = "codex";
	pub const DEFAULT_MODEL: &'static str = "gpt-5.1";
	pub const DEFAULT_EFFORT: &'static str = "high";

	pub fn new(additional_prompts: Vec<String>) -> Self {
		Self::with_settings(additional_prompts, Self::DEFAULT_MODEL, Self::DEFAULT_EFFORT)
	}

	pub fn with_settings(additional_prompts: Vec<String>, model: impl Into<String>, effort: impl Into<String>) -> Self {
		let effort = effort.into();

		Self {
			additional_prompts,
			effort: effort.clone(),
			executor: DefaultCodexExecutor::new(model, effort),
		}
	}
}

impl<E: CodexExecutor> CodexCliBasedI18nTranslator<E> {
	pub fn with_executor(executor: E) -> Self {
		Self {
			additional_prompts: Vec::new(),
			effort: CodexCliBasedI18nTranslator::<DefaultCodexExecutor>::DEFAULT_EFFORT.to_string(),
			executor,
		}
	}

	pub fn with_additional_prompts(mut self, additional_prompts: Vec<String>) -> Self {
		self.additional_prompts = additional_prompts;
		self
	}

	pub fn with_effort(mut self, effort: impl Into<String>) -> Self {
		self.effort = effort.into();
		self
	}
}

#[async_trait]
impl<E: CodexExecutor> I18nTranslator for CodexCliBasedI18nTranslator<E> {
	fn translator_id(&self) -> &str {
		"codex"
	}

	async fn translate_values(
		&self,
		from_language: &str,
		to_language: &str,
		values: Vec<String>,
		format: ContentFormat,
	) -> J18nResult<Vec<String>> {
		let values_for_prompt_serialized = serde_json::to_string(&values)
			.map_err(|e| J18nError::translator(format!("failed to serialize prompt array: {e}")))?;
		let prompt = build_prompt(
			from_language,
			to_language,
			&self.additional_prompts,
			&self.effort,
			format,
			&values_for_prompt_serialized,
		);
		let response = self.executor.execute(&prompt).await?;

		Ok(response
			.split(ENTRY_SEPARATOR)
			.map(|s| s.trim().to_string())
			.filter(|s| !s.is_empty())
			.collect())
	}
}

fn build_prompt(
	from_language: &str,
	to_language: &str,
	additional_prompts: &[String],
	effort: &str,
	format: ContentFormat,
	values_for_prompt_serialized: &str,
) -> String {
	let mut lines: Vec<String> = vec![format!("Use {effort} reasoning effort.")];

	lines.extend(instruction_lines(from_language, to_language, format));
	lines.push("DO NOT remove, skip or modify placeholders, like [1], [2], [3], etc.".to_string());

	for prompt in additional_prompts {
		lines.push(prompt.clone());
	}

	lines.push("Once again, DO NOT remove placeholders like '[1]', '[2]', '[3]', '[4]', etc.".to_string());
	lines.extend(answer_lines(format));
	lines.push(values_for_prompt_serialized.to_string());

	lines.join("\n")
}

fn instruction_lines(from_language: &str, to_language: &str, format: ContentFormat) -> Vec<String> {
	match format {
		ContentFormat::Json => vec![
			format!("Translate the values in the following JSON array, from {from_language} to {to_language}."),
			"DO NOT remove or modify HTML tags.".to_string(),
		],
		ContentFormat::Markdown => vec![
			format!("Translate the Markdown/MDX document(s) in the following JSON array, from {from_language} to {to_language}."),
			"Preserve ALL Markdown and MDX syntax exactly: headings, lists, tables, blockquotes, emphasis, and horizontal rules.".to_string(),
			"DO NOT translate or alter fenced or inline code, code block contents, URLs, link targets, image paths, HTML/JSX tags and attributes, JSX/React component names, or import/export statements.".to_string(),
			"For YAML front matter, translate only human-readable string values (e.g. title, description); never translate front matter keys.".to_string(),
			"Translate only human-readable prose: headings, paragraphs, list items, table cells, link text, and image alt text.".to_string(),
			"DO NOT add, remove, or reflow whitespace, blank lines, or indentation beyond what translating the prose itself requires.".to_string(),
		],
	}
}

fn answer_lines(format: ContentFormat) -> Vec<String> {
	match format {
		ContentFormat::Json => vec![
			format!(
				"Answer ONLY with the translated values, one per line, each separated by the exact string '{ENTRY_SEPARATOR}' on its own line, in the same order as the inputs."
			),
			format!(
				"Do NOT include any other text, explanations, numbering, or formatting — only the translated values separated by '{ENTRY_SEPARATOR}'."
			),
			"The JSON array of values to translate is:".to_string(),
		],
		ContentFormat::Markdown => vec![
			format!(
				"Answer ONLY with the translated document(s), each separated by the exact string '{ENTRY_SEPARATOR}' on its own line, in the same order as the inputs."
			),
			"Do NOT wrap the documents in code fences and do NOT add explanations or commentary.".to_string(),
			"The JSON array of documents to translate is:".to_string(),
		],
	}
}

async fn execute_codex(model: &str, effort: &str, prompt: &str) -> J18nResult<String> {
	let model_arg = format!("--model={model}");
	let effort_override = format!("model_reasoning_effort={effort}");
	let mut command = if cfg!(target_os = "windows") {
		let mut command = Command::new("cmd");

		command.args([
			"/C",
			"codex",
			"exec",
			"--color",
			"never",
			&model_arg,
			"-c",
			&effort_override,
			"-",
		]);
		command
	} else {
		let mut command = Command::new("codex");

		command.args(["exec", "--color", "never", &model_arg, "-c", &effort_override, "-"]);
		command
	};

	command
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());

	let mut child = command
		.spawn()
		.map_err(|e| J18nError::translator(format!("failed to spawn Codex CLI process: {e}")))?;

	if let Some(stdin) = child.stdin.as_mut() {
		stdin
			.write_all(prompt.as_bytes())
			.await
			.map_err(|e| J18nError::translator(format!("failed to write prompt to Codex CLI: {e}")))?;
		stdin
			.shutdown()
			.await
			.map_err(|e| J18nError::translator(format!("failed to close Codex CLI stdin: {e}")))?;
	}

	let output = child
		.wait_with_output()
		.await
		.map_err(|e| J18nError::translator(format!("failed to wait for Codex CLI: {e}")))?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
		let exit_code = output
			.status
			.code()
			.map(|c| c.to_string())
			.unwrap_or_else(|| "<signal>".to_string());

		return Err(J18nError::translator(format!(
			"Codex CLI process exited with code {exit_code}. Stderr: {stderr}"
		)));
	}

	Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::{Arc, Mutex};

	struct MockExecutor {
		captured: Arc<Mutex<Vec<String>>>,
		response: J18nResult<String>,
	}

	impl MockExecutor {
		fn ok(response: impl Into<String>) -> (Self, Arc<Mutex<Vec<String>>>) {
			let captured = Arc::new(Mutex::new(Vec::new()));

			(
				Self {
					captured: Arc::clone(&captured),
					response: Ok(response.into()),
				},
				captured,
			)
		}

		fn err(message: impl Into<String>) -> Self {
			Self {
				captured: Arc::new(Mutex::new(Vec::new())),
				response: Err(J18nError::translator(message.into())),
			}
		}
	}

	#[async_trait]
	impl CodexExecutor for MockExecutor {
		async fn execute(&self, prompt: &str) -> J18nResult<String> {
			self.captured.lock().unwrap().push(prompt.to_string());

			match &self.response {
				Ok(value) => Ok(value.clone()),
				Err(J18nError::Translator(message)) => Err(J18nError::translator(message.clone())),
				Err(_) => Err(J18nError::translator("mock executor failure")),
			}
		}
	}

	const ENGLISH: &str = "English";
	const PORTUGUESE: &str = "Portuguese";

	#[tokio::test]
	async fn translates_values_via_separator() {
		let (executor, captured) = MockExecutor::ok("olá<<<SEP>>>mundo");
		let translator = CodexCliBasedI18nTranslator::with_executor(executor);

		let translated = translator
			.translate_values(
				ENGLISH,
				PORTUGUESE,
				vec!["hello".into(), "world".into()],
				ContentFormat::Json,
			)
			.await
			.unwrap();

		assert_eq!(translated, vec!["olá".to_string(), "mundo".to_string()]);
		assert_eq!(captured.lock().unwrap().len(), 1);
	}

	#[tokio::test]
	async fn propagates_executor_errors() {
		let executor = MockExecutor::err("boom");
		let translator = CodexCliBasedI18nTranslator::with_executor(executor);

		let err = translator
			.translate_values(ENGLISH, PORTUGUESE, vec!["a".into()], ContentFormat::Json)
			.await
			.unwrap_err();

		match err {
			J18nError::Translator(message) => assert!(message.contains("boom")),
			other => panic!("unexpected error: {other:?}"),
		}
	}

	#[tokio::test]
	async fn prompt_includes_default_effort_directive() {
		let (executor, captured) = MockExecutor::ok("Olá");
		let translator = CodexCliBasedI18nTranslator::with_executor(executor);

		translator
			.translate_values(ENGLISH, PORTUGUESE, vec!["Hi".into()], ContentFormat::Json)
			.await
			.unwrap();

		let prompts = captured.lock().unwrap();
		let prompt = &prompts[0];

		assert!(prompt.contains("Use high reasoning effort."));
	}

	#[tokio::test]
	async fn prompt_reflects_custom_effort() {
		let (executor, captured) = MockExecutor::ok("Olá");
		let translator = CodexCliBasedI18nTranslator::with_executor(executor).with_effort("low");

		translator
			.translate_values(ENGLISH, PORTUGUESE, vec!["Hi".into()], ContentFormat::Json)
			.await
			.unwrap();

		let prompts = captured.lock().unwrap();
		let prompt = &prompts[0];

		assert!(prompt.contains("Use low reasoning effort."));
	}

	#[test]
	fn translator_id_is_codex() {
		let translator = CodexCliBasedI18nTranslator::new(Vec::new());

		assert_eq!(translator.translator_id(), "codex");
	}

	#[tokio::test]
	async fn additional_prompts_are_injected_between_placeholder_warnings() {
		let (executor, captured) = MockExecutor::ok("X");
		let translator = CodexCliBasedI18nTranslator::with_executor(executor)
			.with_additional_prompts(vec!["INJECTED-CONTEXT-A".to_string(), "INJECTED-CONTEXT-B".to_string()]);

		translator
			.translate_values(ENGLISH, PORTUGUESE, vec!["x".into()], ContentFormat::Json)
			.await
			.unwrap();

		let prompts = captured.lock().unwrap();
		let prompt = &prompts[0];
		let placeholder_position = prompt
			.find("DO NOT remove, skip or modify placeholders")
			.expect("first placeholder warning must be present");
		let injected_a_position = prompt.find("INJECTED-CONTEXT-A").expect("injected line A missing");
		let injected_b_position = prompt.find("INJECTED-CONTEXT-B").expect("injected line B missing");
		let reminder_position = prompt
			.find("Once again, DO NOT remove placeholders")
			.expect("placeholder reminder must be present");

		assert!(placeholder_position < injected_a_position);
		assert!(injected_a_position < injected_b_position);
		assert!(injected_b_position < reminder_position);
	}

	#[tokio::test]
	async fn markdown_prompt_instructs_to_preserve_syntax_and_omits_json_array_framing() {
		let (executor, captured) = MockExecutor::ok("# Olá");
		let translator = CodexCliBasedI18nTranslator::with_executor(executor);

		translator
			.translate_values(ENGLISH, PORTUGUESE, vec!["# Hi".into()], ContentFormat::Markdown)
			.await
			.unwrap();

		let prompts = captured.lock().unwrap();
		let prompt = &prompts[0];

		assert!(prompt.contains("Translate the Markdown/MDX document(s)"));
		assert!(prompt.contains("Preserve ALL Markdown and MDX syntax"));
		assert!(prompt.contains("Do NOT wrap the documents in code fences"));
		assert!(!prompt.contains("Translate the values in the following JSON array"));
		assert!(prompt.contains(ENTRY_SEPARATOR));
	}
}
