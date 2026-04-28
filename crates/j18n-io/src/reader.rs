use crate::json_walker::walk_json_tree_to_map;
use j18n_core::{I18nData, I18nDefinition, J18nError, J18nResult};
use serde_json::{Map, Value};
use tokio::fs;

pub async fn read_i18n_data(definition: &I18nDefinition) -> J18nResult<I18nData> {
	let path = &definition.json_file_path;

	if !fs::try_exists(path).await.map_err(|source| J18nError::Io {
		path: path.clone(),
		source,
	})? {
		return Ok(I18nData::empty());
	}

	let raw_bytes = fs::read(path).await.map_err(|source| J18nError::Io {
		path: path.clone(),
		source,
	})?;
	let mut json_dict: Map<String, Value> = serde_json::from_slice(&raw_bytes).map_err(|source| J18nError::Json {
		path: path.clone(),
		source,
	})?;

	json_dict.remove("sample");

	let walked_tree_map = walk_json_tree_to_map(&json_dict);

	Ok(I18nData {
		json_dict,
		walked_tree_map,
	})
}
