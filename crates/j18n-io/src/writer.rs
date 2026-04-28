use j18n_core::{I18nDefinition, J18nError, J18nResult};
use serde::Serialize;
use serde_json::{Map, Value};
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub async fn write_i18n_tree_map(
	definition: &I18nDefinition,
	reference_json_dict: &Map<String, Value>,
	initial_json_dict: Map<String, Value>,
	json_tree_map_list: &[Vec<(String, String)>],
) -> J18nResult<()> {
	let mut translated_json_dict = initial_json_dict;

	for batch in json_tree_map_list {
		for (key, value) in batch {
			translated_json_dict = change_i18n_property(translated_json_dict, key, value);
		}
	}

	let cleaned_json_dict = remove_keys_absent_from_reference_dict(reference_json_dict, &translated_json_dict);
	let serialized = serialize_pretty(&cleaned_json_dict).map_err(|source| J18nError::Json {
		path: definition.json_file_path.clone(),
		source,
	})?;

	if let Some(parent) = definition.json_file_path.parent() {
		fs::create_dir_all(parent).await.map_err(|source| J18nError::Io {
			path: parent.to_path_buf(),
			source,
		})?;
	}

	let mut file = fs::File::create(&definition.json_file_path)
		.await
		.map_err(|source| J18nError::Io {
			path: definition.json_file_path.clone(),
			source,
		})?;

	file.write_all(serialized.as_bytes())
		.await
		.map_err(|source| J18nError::Io {
			path: definition.json_file_path.clone(),
			source,
		})?;
	file.write_all(b"\n").await.map_err(|source| J18nError::Io {
		path: definition.json_file_path.clone(),
		source,
	})?;

	Ok(())
}

fn change_i18n_property(mut json: Map<String, Value>, key_dot_separated: &str, value: &str) -> Map<String, Value> {
	if let Some((this_part, rest_parts)) = key_dot_separated.split_once('.') {
		let sub_json = match json.remove(this_part) {
			Some(Value::Object(existing)) => existing,
			_ => Map::new(),
		};
		let changed_sub_json = change_i18n_property(sub_json, rest_parts, value);

		json.insert(this_part.to_string(), Value::Object(changed_sub_json));

		return json;
	}

	json.insert(key_dot_separated.to_string(), Value::String(value.to_string()));

	json
}

fn remove_keys_absent_from_reference_dict(
	reference_dict: &Map<String, Value>,
	target_dict: &Map<String, Value>,
) -> Map<String, Value> {
	let mut result = Map::new();

	for (key, target_value) in target_dict {
		let Some(reference_value) = reference_dict.get(key) else {
			continue;
		};

		match (reference_value, target_value) {
			(Value::Object(reference_sub), Value::Object(target_sub)) => {
				result.insert(
					key.clone(),
					Value::Object(remove_keys_absent_from_reference_dict(reference_sub, target_sub)),
				);
			}
			_ => {
				result.insert(key.clone(), target_value.clone());
			}
		}
	}

	result
}

fn serialize_pretty(value: &Map<String, Value>) -> Result<String, serde_json::Error> {
	let formatter = serde_json::ser::PrettyFormatter::with_indent(b"\t");
	let mut buffer = Vec::new();
	let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);

	value.serialize(&mut serializer)?;

	Ok(String::from_utf8(buffer).expect("serde_json always produces valid UTF-8"))
}
