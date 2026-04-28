use crate::hashing::{java_string_hashcode_hex, I18nHashing};
use crate::json_walker::walk_json_tree_to_map;
use j18n_core::{I18nData, J18nError, J18nResult};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub struct I18nHashingCache;

impl I18nHashingCache {
	pub fn compute_hash_cache_from(i18n_data: &I18nData) -> I18nHashing {
		let json_key_to_hash_map = i18n_data
			.walked_tree_map
			.iter()
			.map(|(key, value)| (key.clone(), java_string_hashcode_hex(value)))
			.collect();

		I18nHashing { json_key_to_hash_map }
	}

	pub async fn load_hash_cache_from(path: &Path) -> J18nResult<I18nHashing> {
		if !fs::try_exists(path).await.map_err(|source| J18nError::Io {
			path: path.to_path_buf(),
			source,
		})? {
			return Ok(I18nHashing::default());
		}

		let raw_bytes = fs::read(path).await.map_err(|source| J18nError::Io {
			path: path.to_path_buf(),
			source,
		})?;
		let json_dict: Map<String, Value> = serde_json::from_slice(&raw_bytes).map_err(|source| J18nError::Json {
			path: path.to_path_buf(),
			source,
		})?;
		let walked = walk_json_tree_to_map(&json_dict);
		let json_key_to_hash_map: HashMap<String, String> = walked.into_iter().collect();

		Ok(I18nHashing { json_key_to_hash_map })
	}

	pub async fn save_hash_cache_to(hashing: &I18nHashing, path: &Path) -> J18nResult<()> {
		let mut sorted: Vec<(&String, &String)> = hashing.json_key_to_hash_map.iter().collect();

		sorted.sort_by(|a, b| a.0.cmp(b.0));

		let mut ordered = Map::new();

		for (key, value) in sorted {
			ordered.insert(key.clone(), Value::String(value.clone()));
		}

		let serialized = serialize_pretty(&ordered).map_err(|source| J18nError::Json {
			path: path.to_path_buf(),
			source,
		})?;

		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).await.map_err(|source| J18nError::Io {
				path: parent.to_path_buf(),
				source,
			})?;
		}

		let mut file = fs::File::create(path).await.map_err(|source| J18nError::Io {
			path: path.to_path_buf(),
			source,
		})?;

		file.write_all(serialized.as_bytes())
			.await
			.map_err(|source| J18nError::Io {
				path: path.to_path_buf(),
				source,
			})?;
		file.write_all(b"\n").await.map_err(|source| J18nError::Io {
			path: path.to_path_buf(),
			source,
		})?;

		Ok(())
	}
}

fn serialize_pretty(value: &Map<String, Value>) -> Result<String, serde_json::Error> {
	let formatter = serde_json::ser::PrettyFormatter::with_indent(b"\t");
	let mut buffer = Vec::new();
	let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);

	value.serialize(&mut serializer)?;

	Ok(String::from_utf8(buffer).expect("serde_json always produces valid UTF-8"))
}
