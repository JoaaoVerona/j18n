use serde_json::{Map, Value};

pub fn walk_json_tree_to_map(json: &Map<String, Value>) -> Vec<(String, String)> {
	let mut output = Vec::new();

	walk(json, "", &mut output);

	output
}

fn walk(json: &Map<String, Value>, key_prefix: &str, output: &mut Vec<(String, String)>) {
	for (key, value) in json {
		match value {
			Value::String(string_value) => {
				output.push((format!("{key_prefix}{key}"), string_value.clone()));
			}
			Value::Object(object_value) => {
				let new_prefix = format!("{key_prefix}{key}.");

				walk(object_value, &new_prefix, output);
			}
			_ => {}
		}
	}
}
