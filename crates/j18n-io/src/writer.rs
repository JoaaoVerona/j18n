use crate::compare::natural_key_cmp;
use j18n_core::{I18nDefinition, J18nError, J18nResult};
use serde::Serialize;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use tokio::fs;

pub async fn write_i18n_tree_map(
	definition: &I18nDefinition,
	indent: &[u8],
	reference_json_dict: &Map<String, Value>,
	target_json_dict: &Map<String, Value>,
	json_tree_map_list: &[Vec<(String, String)>],
) -> J18nResult<()> {
	// The skeleton mirrors the reference's structure (so stale target keys drop
	// out and every translatable key has a home) but keeps the target file's
	// existing key order. User-made ordering is therefore preserved verbatim;
	// only keys the target doesn't have yet are placed in natural-sorted order.
	let mut translated_json_dict = build_ordered_dict(reference_json_dict, target_json_dict);

	for batch in json_tree_map_list {
		for (key, value) in batch {
			change_i18n_property(&mut translated_json_dict, key, value);
		}
	}

	let mut serialized = serialize_pretty(&translated_json_dict, indent).map_err(|source| J18nError::Json {
		path: definition.file.clone(),
		source,
	})?;

	serialized.push('\n');

	if let Some(parent) = definition.file.parent() {
		fs::create_dir_all(parent).await.map_err(|source| J18nError::Io {
			path: parent.to_path_buf(),
			source,
		})?;
	}

	// `fs::write` opens, writes, and closes in one shot; `tokio::fs::File` does
	// not flush on drop, so a `File` + `write_all` without an explicit flush can
	// leave a truncated/empty file on Linux.
	fs::write(&definition.file, serialized.as_bytes())
		.await
		.map_err(|source| J18nError::Io {
			path: definition.file.clone(),
			source,
		})
}

/// Writes a translated Markdown/MDX document verbatim to the target file,
/// creating parent directories as needed. The body is written as-is except that
/// a single trailing newline is guaranteed (POSIX-text convention) — translator
/// backends trim their output, so this restores the customary final newline
/// without appending blank lines.
pub async fn write_markdown_file(definition: &I18nDefinition, body: &str) -> J18nResult<()> {
	if let Some(parent) = definition.file.parent() {
		if !parent.as_os_str().is_empty() {
			fs::create_dir_all(parent).await.map_err(|source| J18nError::Io {
				path: parent.to_path_buf(),
				source,
			})?;
		}
	}

	let mut contents = body.to_string();

	if !contents.ends_with('\n') {
		contents.push('\n');
	}

	// `fs::write` opens, writes, and closes in one shot. We avoid `fs::File` +
	// `write_all` here because `tokio::fs::File` does not flush on drop, so
	// buffered bytes could be lost without an explicit `flush().await` (this
	// manifested as truncated/empty files on Linux while passing on Windows).
	fs::write(&definition.file, contents.as_bytes())
		.await
		.map_err(|source| J18nError::Io {
			path: definition.file.clone(),
			source,
		})
}

/// Writes a translated value into the skeleton produced by [`build_ordered_dict`],
/// which already contains every reference key. Existing entries are overwritten
/// **in place**, so a key never moves: editing a value never reorders the file.
fn change_i18n_property(json: &mut Map<String, Value>, key_dot_separated: &str, value: &str) {
	// 1. Leaf: an existing key whose value is not an object is overwritten
	//    directly. `Map::insert` updates an existing key without moving it.
	//    Reached both for genuinely flat string entries and for the final
	//    segment of a descent (e.g. "message" inside a Docusaurus entry).
	if json
		.get(key_dot_separated)
		.is_some_and(|existing| !existing.is_object())
	{
		json.insert(key_dot_separated.to_string(), Value::String(value.to_string()));

		return;
	}

	// 2. Route into the LONGEST existing object-valued key that is a dot-boundary
	//    prefix of the path, descending in place. This preserves keys that
	//    contain literal dots — e.g. Docusaurus / i18next "flat" keys like
	//    "theme.docs.paginator.next" or "link.title.More" — instead of re-nesting
	//    them into separate objects, and it routes through genuine object levels
	//    (e.g. "section.a") without disturbing sibling order.
	if let Some(prefix) = longest_object_prefix(json, key_dot_separated) {
		let remainder = key_dot_separated[prefix.len()..].trim_start_matches('.').to_string();

		if let Some(Value::Object(sub_json)) = json.get_mut(&prefix) {
			change_i18n_property(sub_json, &remainder, value);
		}
	}
}

/// Returns the longest key in `json` whose value is an object and that is a
/// strict dot-boundary prefix of `path` (so `path == "<key>.<rest>"`). Used to
/// descend into existing keys that themselves contain dots before falling back
/// to creating new nesting.
fn longest_object_prefix(json: &Map<String, Value>, path: &str) -> Option<String> {
	let mut best: Option<&String> = None;

	for key in json.keys() {
		if path.len() <= key.len() || !path.starts_with(key.as_str()) || path.as_bytes()[key.len()] != b'.' {
			continue;
		}

		if !matches!(json.get(key), Some(Value::Object(_))) {
			continue;
		}

		let better = match best {
			Some(current) => key.len() > current.len(),
			None => true,
		};

		if better {
			best = Some(key);
		}
	}

	best.cloned()
}

/// Builds the output skeleton from the reference's structure while honouring the
/// target file's existing key order:
///
/// - Reference keys the target already has appear in the **target's** order, with
///   their current value kept (so untouched translations are preserved verbatim).
/// - Reference keys the target lacks are inserted at their **natural-sorted**
///   position, so brand-new entries land sorted without disturbing existing ones.
/// - Target keys absent from the reference are dropped (stale-key pruning).
///
/// Nested objects recurse, so the same rules apply at every level. A subtree that
/// is entirely new (no counterpart in the target) is emitted fully natural-sorted.
fn build_ordered_dict(reference: &Map<String, Value>, target: &Map<String, Value>) -> Map<String, Value> {
	let mut ordered: Vec<(String, Value)> = Vec::with_capacity(reference.len());

	// 1. Keep every reference key the target already has, in the target's order.
	for (key, target_value) in target {
		let Some(reference_value) = reference.get(key) else {
			continue;
		};

		let value = match reference_value {
			Value::Object(reference_sub) => {
				let empty = Map::new();
				let target_sub = match target_value {
					Value::Object(sub) => sub,
					_ => &empty,
				};

				Value::Object(build_ordered_dict(reference_sub, target_sub))
			}
			// Reference says leaf: keep the target's existing value, unless the
			// target diverged structurally (object where a leaf is expected), in
			// which case fall back to the reference value.
			_ => match target_value {
				Value::Object(_) => reference_value.clone(),
				_ => target_value.clone(),
			},
		};

		ordered.push((key.clone(), value));
	}

	// 2. Insert reference keys the target lacks at their natural-sorted position.
	for (key, reference_value) in reference {
		if target.contains_key(key) {
			continue;
		}

		let value = sort_new_value(reference_value);
		let position = ordered
			.iter()
			.position(|(existing, _)| natural_key_cmp(existing, key) == Ordering::Greater);

		match position {
			Some(index) => ordered.insert(index, (key.clone(), value)),
			None => ordered.push((key.clone(), value)),
		}
	}

	ordered.into_iter().collect()
}

/// Recursively natural-sorts the keys of a brand-new value (one with no existing
/// counterpart in the target), so freshly added subtrees are emitted sorted.
fn sort_new_value(value: &Value) -> Value {
	match value {
		Value::Object(map) => {
			let mut entries: Vec<(&String, &Value)> = map.iter().collect();

			entries.sort_by(|(a, _), (b, _)| natural_key_cmp(a, b));

			Value::Object(
				entries
					.into_iter()
					.map(|(key, value)| (key.clone(), sort_new_value(value)))
					.collect(),
			)
		}
		Value::Array(items) => Value::Array(items.iter().map(sort_new_value).collect()),
		other => other.clone(),
	}
}

fn serialize_pretty(value: &Map<String, Value>, indent: &[u8]) -> Result<String, serde_json::Error> {
	let formatter = serde_json::ser::PrettyFormatter::with_indent(indent);
	let mut buffer = Vec::new();
	let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);

	value.serialize(&mut serializer)?;

	Ok(String::from_utf8(buffer).expect("serde_json always produces valid UTF-8"))
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;
	use tokio::fs;

	fn parse(json: &str) -> Map<String, Value> {
		serde_json::from_str(json).unwrap()
	}

	fn definition_in(dir: &TempDir, code: &str) -> I18nDefinition {
		let file = dir.path().join(format!("{code}.json"));
		let id = format!("{code}.json");

		I18nDefinition {
			file,
			id,
			language: code.to_string(),
		}
	}

	#[tokio::test]
	async fn writes_with_supplied_indentation_and_trailing_newline() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"a": "x"}"#);
		let initial = reference.clone();
		let translations = vec![vec![("a".to_string(), "y".to_string())]];

		write_i18n_tree_map(&definition, b"\t", &reference, &initial, &translations)
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();

		assert_eq!(written, "{\n\t\"a\": \"y\"\n}\n");
	}

	#[tokio::test]
	async fn writes_with_two_space_indent_when_requested() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"a": "x"}"#);
		let initial = reference.clone();

		write_i18n_tree_map(&definition, b"  ", &reference, &initial, &[])
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();

		assert_eq!(written, "{\n  \"a\": \"x\"\n}\n");
	}

	#[tokio::test]
	async fn applies_dot_separated_keys_into_nested_objects() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"section": {"a": "X", "b": "Y"}}"#);
		let initial = reference.clone();
		let translations = vec![vec![
			("section.a".to_string(), "AA".to_string()),
			("section.b".to_string(), "BB".to_string()),
		]];

		write_i18n_tree_map(&definition, b"\t", &reference, &initial, &translations)
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();
		let parsed: Map<String, Value> = serde_json::from_str(&written).unwrap();

		assert_eq!(parsed["section"]["a"], "AA");
		assert_eq!(parsed["section"]["b"], "BB");
	}

	#[tokio::test]
	async fn prunes_keys_absent_from_reference_dict() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"keep": "K"}"#);
		let initial = parse(r#"{"keep": "K", "stale": "S"}"#);

		write_i18n_tree_map(&definition, b"\t", &reference, &initial, &[])
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();
		let parsed: Map<String, Value> = serde_json::from_str(&written).unwrap();

		assert!(parsed.contains_key("keep"));
		assert!(!parsed.contains_key("stale"));
	}

	#[tokio::test]
	async fn prunes_nested_keys_absent_from_reference_dict() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"section": {"keep": "K"}}"#);
		let initial = parse(r#"{"section": {"keep": "K", "stale": "S"}}"#);

		write_i18n_tree_map(&definition, b"\t", &reference, &initial, &[])
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();
		let parsed: Map<String, Value> = serde_json::from_str(&written).unwrap();

		assert!(parsed["section"].as_object().unwrap().contains_key("keep"));
		assert!(!parsed["section"].as_object().unwrap().contains_key("stale"));
	}

	// When the target has no prior entries (fresh file), every key is "new" and so
	// the whole output comes out in natural-sorted order.
	#[tokio::test]
	async fn new_top_level_keys_are_inserted_in_natural_order() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"zebra": "Z", "apple": "A", "mango": "M"}"#);
		let target = Map::new();

		write_i18n_tree_map(&definition, b"\t", &reference, &target, &[])
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();
		let apple_pos = written.find("\"apple\"").unwrap();
		let mango_pos = written.find("\"mango\"").unwrap();
		let zebra_pos = written.find("\"zebra\"").unwrap();

		assert!(apple_pos < mango_pos);
		assert!(mango_pos < zebra_pos);
	}

	#[tokio::test]
	async fn new_numeric_keys_are_inserted_in_natural_order() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"0":"a","1":"b","10":"c","11":"d","2":"e"}"#);
		let target = Map::new();

		write_i18n_tree_map(&definition, b"\t", &reference, &target, &[])
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();
		let positions: Vec<usize> = ["\"0\"", "\"1\"", "\"2\"", "\"10\"", "\"11\""]
			.iter()
			.map(|key| written.find(key).unwrap())
			.collect();

		for window in positions.windows(2) {
			assert!(window[0] < window[1], "natural order violated: {:?}", positions);
		}
	}

	#[tokio::test]
	async fn new_camel_case_keys_are_inserted_with_uppercase_before_lowercase() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"types": "T", "typeSelection": "S"}"#);
		let target = Map::new();

		write_i18n_tree_map(&definition, b"\t", &reference, &target, &[])
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();
		let type_selection_pos = written.find("\"typeSelection\"").unwrap();
		let types_pos = written.find("\"types\"").unwrap();

		assert!(type_selection_pos < types_pos, "expected typeSelection before types");
	}

	#[tokio::test]
	async fn new_nested_subtrees_are_emitted_sorted() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"section": {"zebra": "Z", "apple": "A"}}"#);
		let target = Map::new();

		write_i18n_tree_map(&definition, b"\t", &reference, &target, &[])
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();
		let apple_pos = written.find("\"apple\"").unwrap();
		let zebra_pos = written.find("\"zebra\"").unwrap();

		assert!(apple_pos < zebra_pos);
	}

	// The core guarantee: an existing target file's key order is never rearranged,
	// even when it is not in natural order.
	#[tokio::test]
	async fn preserves_existing_target_key_order_without_resorting() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"apple": "A", "mango": "M", "zebra": "Z"}"#);
		let target = parse(r#"{"zebra": "ZZ", "apple": "AA", "mango": "MM"}"#);

		write_i18n_tree_map(&definition, b"\t", &reference, &target, &[])
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();
		let zebra_pos = written.find("\"zebra\"").unwrap();
		let apple_pos = written.find("\"apple\"").unwrap();
		let mango_pos = written.find("\"mango\"").unwrap();

		assert!(zebra_pos < apple_pos, "existing order zebra→apple must be kept");
		assert!(apple_pos < mango_pos, "existing order apple→mango must be kept");
	}

	// A brand-new key lands in its natural-sorted position relative to the
	// existing (sorted) siblings, and the existing entries keep their values.
	#[tokio::test]
	async fn new_key_is_inserted_at_sorted_position_among_existing_keys() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"apple": "A", "mango": "M", "zebra": "Z"}"#);
		let target = parse(r#"{"apple": "AA", "zebra": "ZZ"}"#);
		let translations = vec![vec![("mango".to_string(), "MM".to_string())]];

		write_i18n_tree_map(&definition, b"\t", &reference, &target, &translations)
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();
		let parsed: Map<String, Value> = serde_json::from_str(&written).unwrap();

		assert_eq!(parsed["apple"], "AA");
		assert_eq!(parsed["zebra"], "ZZ");
		assert_eq!(parsed["mango"], "MM");

		let apple_pos = written.find("\"apple\"").unwrap();
		let mango_pos = written.find("\"mango\"").unwrap();
		let zebra_pos = written.find("\"zebra\"").unwrap();

		assert!(
			apple_pos < mango_pos && mango_pos < zebra_pos,
			"mango must be inserted between apple and zebra"
		);
	}

	// Editing an existing value must overwrite it in place, never move the key.
	#[tokio::test]
	async fn editing_a_value_keeps_the_key_in_place() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"a": "A", "b": "B"}"#);
		let target = parse(r#"{"b": "OLD_B", "a": "OLD_A"}"#);
		let translations = vec![vec![("b".to_string(), "NEW_B".to_string())]];

		write_i18n_tree_map(&definition, b"\t", &reference, &target, &translations)
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();
		let parsed: Map<String, Value> = serde_json::from_str(&written).unwrap();

		assert_eq!(parsed["b"], "NEW_B");
		assert_eq!(parsed["a"], "OLD_A");

		let b_pos = written.find("\"b\"").unwrap();
		let a_pos = written.find("\"a\"").unwrap();

		assert!(b_pos < a_pos, "existing b→a order must be preserved after editing b");
	}

	#[tokio::test]
	async fn creates_parent_directories_when_missing() {
		let dir = TempDir::new().unwrap();
		let nested_dir = dir.path().join("does/not/exist");
		let definition = I18nDefinition {
			file: nested_dir.join("pt.json"),
			id: "pt.json".to_string(),
			language: "pt".to_string(),
		};
		let reference = parse(r#"{"a": "x"}"#);
		let initial = reference.clone();

		write_i18n_tree_map(&definition, b"\t", &reference, &initial, &[])
			.await
			.unwrap();

		assert!(definition.file.exists());
	}

	fn markdown_definition_in(dir: &TempDir, name: &str) -> I18nDefinition {
		let file = dir.path().join(format!("{name}.mdx"));

		I18nDefinition {
			file,
			id: format!("{name}.mdx"),
			language: name.to_string(),
		}
	}

	#[tokio::test]
	async fn markdown_writes_body_verbatim_when_it_already_ends_with_newline() {
		let dir = TempDir::new().unwrap();
		let definition = markdown_definition_in(&dir, "pt");
		let body = "# Título\n\nUm parágrafo com `code`.\n";

		write_markdown_file(&definition, body).await.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();

		assert_eq!(written, body);
	}

	#[tokio::test]
	async fn markdown_appends_single_trailing_newline_when_missing() {
		let dir = TempDir::new().unwrap();
		let definition = markdown_definition_in(&dir, "pt");

		write_markdown_file(&definition, "# Título").await.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();

		assert_eq!(written, "# Título\n");
	}

	#[tokio::test]
	async fn markdown_does_not_add_extra_newline_when_already_present() {
		let dir = TempDir::new().unwrap();
		let definition = markdown_definition_in(&dir, "pt");

		write_markdown_file(&definition, "line\n").await.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();

		assert_eq!(written, "line\n");
	}

	#[tokio::test]
	async fn markdown_creates_parent_directories_when_missing() {
		let dir = TempDir::new().unwrap();
		let definition = I18nDefinition {
			file: dir.path().join("i18n/pt-BR/current/welcome.mdx"),
			id: "i18n/pt-BR/current/welcome.mdx".to_string(),
			language: "Brazilian Portuguese".to_string(),
		};

		write_markdown_file(&definition, "# Olá\n").await.unwrap();

		assert!(definition.file.exists());
		assert_eq!(fs::read_to_string(&definition.file).await.unwrap(), "# Olá\n");
	}

	#[tokio::test]
	async fn markdown_overwrites_existing_target() {
		let dir = TempDir::new().unwrap();
		let definition = markdown_definition_in(&dir, "pt");

		fs::write(&definition.file, "old content\n").await.unwrap();
		write_markdown_file(&definition, "new content\n").await.unwrap();

		assert_eq!(fs::read_to_string(&definition.file).await.unwrap(), "new content\n");
	}

	// Docusaurus / i18next "flat" translation files use single keys that contain
	// literal dots, mapping to a `{ "message": ... }` object. The translated
	// value must land inside that flat key, not re-nested into a new object tree.
	#[tokio::test]
	async fn translates_flat_dotted_key_in_place_without_renesting() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"theme.docs.paginator.next": {"message": "Next"}}"#);
		let initial = reference.clone();
		let translations = vec![vec![(
			"theme.docs.paginator.next.message".to_string(),
			"Próximo".to_string(),
		)]];

		write_i18n_tree_map(&definition, b"\t", &reference, &initial, &translations)
			.await
			.unwrap();

		let written = fs::read_to_string(&definition.file).await.unwrap();
		let parsed: Map<String, Value> = serde_json::from_str(&written).unwrap();

		// The flat key is preserved verbatim and its message is translated...
		assert_eq!(parsed["theme.docs.paginator.next"]["message"], "Próximo");
		// ...and no stray nested "theme" object tree was created.
		assert!(!parsed.contains_key("theme"));
		assert_eq!(parsed.len(), 1);
	}

	#[tokio::test]
	async fn translates_flat_dotted_key_with_spaces_and_many_dots() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"link.item.label.Go to Skiley": {"message": "Go to Skiley"}}"#);
		let initial = reference.clone();
		let translations = vec![vec![(
			"link.item.label.Go to Skiley.message".to_string(),
			"Ir para o Skiley".to_string(),
		)]];

		write_i18n_tree_map(&definition, b"\t", &reference, &initial, &translations)
			.await
			.unwrap();

		let parsed: Map<String, Value> =
			serde_json::from_str(&fs::read_to_string(&definition.file).await.unwrap()).unwrap();

		assert_eq!(parsed["link.item.label.Go to Skiley"]["message"], "Ir para o Skiley");
		assert!(!parsed.contains_key("link"));
	}

	#[tokio::test]
	async fn flat_and_dotless_keys_translate_side_by_side() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"copyright": {"message": "Copyright"}, "link.title.More": {"message": "More"}}"#);
		let initial = reference.clone();
		let translations = vec![vec![
			("copyright.message".to_string(), "Direitos autorais".to_string()),
			("link.title.More.message".to_string(), "Mais".to_string()),
		]];

		write_i18n_tree_map(&definition, b"\t", &reference, &initial, &translations)
			.await
			.unwrap();

		let parsed: Map<String, Value> =
			serde_json::from_str(&fs::read_to_string(&definition.file).await.unwrap()).unwrap();

		assert_eq!(parsed["copyright"]["message"], "Direitos autorais");
		assert_eq!(parsed["link.title.More"]["message"], "Mais");
		assert!(!parsed.contains_key("link"));
	}

	#[tokio::test]
	async fn nested_dictionaries_still_route_through_object_levels() {
		let dir = TempDir::new().unwrap();
		let definition = definition_in(&dir, "pt");
		let reference = parse(r#"{"section": {"a": "A", "nested": {"b": "B"}}}"#);
		let initial = reference.clone();
		let translations = vec![vec![
			("section.a".to_string(), "AA".to_string()),
			("section.nested.b".to_string(), "BB".to_string()),
		]];

		write_i18n_tree_map(&definition, b"\t", &reference, &initial, &translations)
			.await
			.unwrap();

		let parsed: Map<String, Value> =
			serde_json::from_str(&fs::read_to_string(&definition.file).await.unwrap()).unwrap();

		assert_eq!(parsed["section"]["a"], "AA");
		assert_eq!(parsed["section"]["nested"]["b"], "BB");
	}
}
