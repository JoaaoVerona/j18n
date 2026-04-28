use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct I18nDefinition {
	pub file: PathBuf,
	pub language: String,
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn empty_i18n_data_has_no_entries() {
		let data = I18nData::empty();

		assert!(data.json_dict.is_empty());
		assert!(data.walked_tree_map.is_empty());
		assert_eq!(data.walked_tree_keys().count(), 0);
	}

	#[test]
	fn walked_tree_keys_iterates_keys_in_order() {
		let data = I18nData {
			json_dict: Default::default(),
			walked_tree_map: vec![("a".into(), "1".into()), ("b.c".into(), "2".into())],
		};
		let keys: Vec<&str> = data.walked_tree_keys().collect();

		assert_eq!(keys, vec!["a", "b.c"]);
	}

	#[test]
	fn i18n_definition_deserializes_from_json() {
		let json = r#"{ "file": "locales/en.json", "language": "English" }"#;
		let definition: I18nDefinition = serde_json::from_str(json).unwrap();

		assert_eq!(definition.file, PathBuf::from("locales/en.json"));
		assert_eq!(definition.language, "English");
	}

	#[test]
	fn i18n_definition_serializes_to_json_with_expected_keys() {
		let definition = I18nDefinition {
			file: PathBuf::from("locales/pt.json"),
			language: "Brazilian Portuguese".to_string(),
		};
		let serialized = serde_json::to_string(&definition).unwrap();

		assert!(serialized.contains("\"file\""));
		assert!(serialized.contains("\"language\""));
		assert!(serialized.contains("Brazilian Portuguese"));
	}
}
