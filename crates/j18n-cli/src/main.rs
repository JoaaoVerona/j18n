mod args;
mod config;

use anyhow::{Context, Result};
use args::{Cli, Command, CommandArgs, InitArgs};
use clap::Parser;
use config::{I18nToolConfig, TranslatorKind};
use j18n_claude_code::ClaudeCodeBasedI18nTranslator;
use j18n_core::{GenerationMode, I18nDefinition, Language};
use j18n_gemini_api::GeminiApiI18nTranslator;
use j18n_generator::I18nGenerator;
use j18n_translator::I18nTranslator;
use j18n_validator::TranslationValidator;
use std::path::{Path, PathBuf};
use tracing::info;
use tracing_subscriber::EnvFilter;

const SKELETON_CONFIG: &str = "{\n\t\"baseDirectory\": \"\",\n\t\"referenceI18n\": \"en\",\n\t\"generateI18nFor\": [],\n\t\"translator\": \"claude-code\"\n}\n";

#[tokio::main]
async fn main() -> Result<()> {
	init_logging();

	let cli = Cli::parse();

	match cli.command {
		Command::Init(args) => init(args).await,
		Command::Sync(args) => run(args, GenerationMode::Sync).await,
		Command::Regenerate(args) => run(args, GenerationMode::Regenerate).await,
	}
}

async fn init(args: InitArgs) -> Result<()> {
	if tokio::fs::try_exists(&args.path)
		.await
		.with_context(|| format!("failed to stat \"{}\"", args.path.display()))?
	{
		anyhow::bail!("refusing to overwrite existing file at \"{}\"", args.path.display());
	}

	if let Some(parent) = args.path.parent() {
		if !parent.as_os_str().is_empty() {
			tokio::fs::create_dir_all(parent)
				.await
				.with_context(|| format!("failed to create directory \"{}\"", parent.display()))?;
		}
	}

	tokio::fs::write(&args.path, SKELETON_CONFIG)
		.await
		.with_context(|| format!("failed to write \"{}\"", args.path.display()))?;

	info!("Created skeleton config at \"{}\"", args.path.display());

	Ok(())
}

async fn run(args: CommandArgs, mode: GenerationMode) -> Result<()> {
	for config_path in &args.configs {
		let config = config::load_config(config_path)?;
		let (reference_i18n, generated_i18ns) = build_definitions(config_path, &config)
			.with_context(|| format!("invalid config \"{}\"", config_path.display()))?;
		let translator: Box<dyn I18nTranslator> = match config.translator {
			TranslatorKind::ClaudeCode => Box::new(ClaudeCodeBasedI18nTranslator::new()),
			TranslatorKind::GeminiApi => Box::new(GeminiApiI18nTranslator::new()?),
		};

		I18nGenerator::execute(translator.as_ref(), &reference_i18n, &generated_i18ns, mode).await?;
		TranslationValidator::validate_translations(&reference_i18n, &generated_i18ns).await?;
	}

	Ok(())
}

fn build_definitions(config_path: &Path, config: &I18nToolConfig) -> Result<(I18nDefinition, Vec<I18nDefinition>)> {
	let reference_language = Language::from_iso_639_code(&config.reference_i18n)
		.with_context(|| format!("invalid referenceI18n \"{}\"", config.reference_i18n))?;
	let base_dir = resolve_base_dir(config_path, &config.base_directory);
	let reference_i18n = I18nDefinition::from_base_dir(&base_dir, reference_language);
	let generated_i18ns = build_target_definitions(&base_dir, &config.generate_i18n_for)?;

	Ok((reference_i18n, generated_i18ns))
}

fn resolve_base_dir(config_path: &Path, base_directory: &Path) -> PathBuf {
	if base_directory.is_absolute() {
		return base_directory.to_path_buf();
	}

	config_path
		.parent()
		.filter(|parent| !parent.as_os_str().is_empty())
		.map(|parent| parent.join(base_directory))
		.unwrap_or_else(|| base_directory.to_path_buf())
}

fn build_target_definitions(base_dir: &Path, codes: &[String]) -> Result<Vec<I18nDefinition>> {
	let mut definitions = Vec::with_capacity(codes.len());

	for code in codes {
		let language =
			Language::from_iso_639_code(code).with_context(|| format!("invalid generateI18nFor entry \"{code}\""))?;

		definitions.push(I18nDefinition::from_base_dir(base_dir, language));
	}

	Ok(definitions)
}

fn init_logging() {
	let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

	tracing_subscriber::fmt()
		.with_env_filter(env_filter)
		.with_target(false)
		.with_writer(std::io::stderr)
		.init();
}
