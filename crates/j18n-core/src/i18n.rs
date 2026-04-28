use crate::language::Language;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct I18nDefinition {
	pub json_file_path: PathBuf,
	pub language: Language,
}

impl I18nDefinition {
	pub fn from_base_dir(base_dir: impl AsRef<Path>, language: Language) -> Self {
		let json_file_path = base_dir.as_ref().join(format!("{}.json", language.iso_639_code()));

		Self {
			json_file_path,
			language,
		}
	}
}

#[derive(Clone, Debug, Default)]
pub struct I18nData {
	pub json_dict: Map<String, Value>,
	pub walked_tree_map: Vec<(String, String)>,
}

impl I18nData {
	pub fn empty() -> Self {
		Self::default()
	}

	pub fn walked_tree_keys(&self) -> impl Iterator<Item = &str> {
		self.walked_tree_map.iter().map(|(key, _)| key.as_str())
	}
}
