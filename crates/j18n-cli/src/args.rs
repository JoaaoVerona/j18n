use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

pub const DEFAULT_CONFIG_FILE: &str = "j18n.json";

#[derive(Debug, Parser)]
#[command(
	name = "j18n",
	about = "Generate or sync localized i18n JSON files from a reference language using LLMs.",
	version
)]
pub struct Cli {
	#[command(subcommand)]
	pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
	#[command(about = "Create a skeleton JSON configuration file at the given path.")]
	Init(InitArgs),

	#[command(about = "Translate only missing entries or those changed since the last run.")]
	Sync(CommandArgs),

	#[command(about = "Translate every entry in the reference, replacing existing translations.")]
	Regenerate(CommandArgs),

	#[command(
		name = "check",
		about = "Report whether `sync` would translate or prune anything; exit non-zero if so."
	)]
	Check(CommandArgs),

	#[command(
		name = "baseline",
		about = "Record current reference hashes for each target without translating; useful when adopting j18n on a project with pre-existing translations so a follow-up `sync` doesn't re-translate everything."
	)]
	Baseline(CommandArgs),

	#[command(
		name = "install-git-hook",
		about = "Install a `pre-commit` hook in the current repo that runs `j18n check` against the configured files."
	)]
	InstallGitHook(CommandArgs),
}

#[derive(Args, Debug, Default)]
pub struct InitArgs {
	#[arg(
		short = 'f',
		long = "file",
		value_name = "PATH",
		help = "Path where the skeleton config file will be written. Defaults to \"j18n.json\" in the current directory when omitted."
	)]
	pub path: Option<PathBuf>,
}

impl InitArgs {
	pub fn resolved_path(&self) -> PathBuf {
		self.path.clone().unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_FILE))
	}
}

#[derive(Args, Debug, Default)]
pub struct CommandArgs {
	#[arg(
		short = 'f',
		long = "file",
		value_name = "PATH",
		help = "Path to a JSON configuration file. May be repeated to act on multiple configs. Defaults to \"j18n.json\" in the current directory when omitted."
	)]
	pub configs: Vec<PathBuf>,
}

impl CommandArgs {
	pub fn resolved_configs(&self) -> Vec<PathBuf> {
		if self.configs.is_empty() {
			vec![PathBuf::from(DEFAULT_CONFIG_FILE)]
		} else {
			self.configs.clone()
		}
	}
}
