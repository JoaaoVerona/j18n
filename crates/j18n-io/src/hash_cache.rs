use crate::compare::natural_key_cmp;
use crate::hashing::I18nHashing;
use j18n_core::{J18nError, J18nResult};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

const KEY_VALUE_SEPARATOR: char = '=';
const SECTION_HEADER_OPEN: char = '[';
const SECTION_HEADER_CLOSE: char = ']';

/// INI-style single-file hashing store. Targets are stored in alphabetically
/// sorted sections, one line per `key=hash`, so:
/// - the file lives in one place (single artifact, single git diff target),
/// - load reads the file line-by-line and only retains the requested
///   target's section, and
/// - save stream-rewrites the file to a temp companion and atomically
///   renames it into place, never holding the entire cache in memory.
#[derive(Clone, Debug)]
pub struct I18nHashingStore {
	file_path: PathBuf,
}

impl I18nHashingStore {
	pub fn at(file_path: impl Into<PathBuf>) -> Self {
		Self {
			file_path: file_path.into(),
		}
	}

	pub fn file_path(&self) -> &Path {
		&self.file_path
	}

	/// Streams the hashing for `target_id` from the cache file. Returns an
	/// empty hashing if the file or the target's section is missing.
	pub async fn load(&self, target_id: &str) -> J18nResult<I18nHashing> {
		validate_target_id(target_id)?;

		if !fs::try_exists(&self.file_path).await.map_err(|source| J18nError::Io {
			path: self.file_path.clone(),
			source,
		})? {
			return Ok(I18nHashing::empty());
		}

		let file = fs::File::open(&self.file_path).await.map_err(|source| J18nError::Io {
			path: self.file_path.clone(),
			source,
		})?;
		let mut reader = BufReader::new(file).lines();
		let mut in_section = false;
		let mut hashing = I18nHashing::empty();

		while let Some(line) = reader.next_line().await.map_err(|source| J18nError::Io {
			path: self.file_path.clone(),
			source,
		})? {
			let trimmed = line.trim_end_matches('\r');

			if let Some(section_id) = parse_section_header(trimmed) {
				if in_section {
					break;
				}

				in_section = section_id == target_id;
				continue;
			}

			if !in_section {
				continue;
			}

			if trimmed.is_empty() {
				continue;
			}

			let Some((key, hash)) = trimmed.split_once(KEY_VALUE_SEPARATOR) else {
				return Err(J18nError::validation(format!(
					"malformed line in hash cache \"{}\": missing '=' in \"{trimmed}\"",
					self.file_path.display()
				)));
			};

			hashing.json_key_to_hash_map.insert(key.to_string(), hash.to_string());
		}

		Ok(hashing)
	}

	/// Replaces the hashing for `target_id` in the cache file, preserving
	/// every other target's section verbatim. The file is rewritten to a
	/// `.tmp` sibling and atomically renamed into place.
	pub async fn save(&self, target_id: &str, hashing: &I18nHashing) -> J18nResult<()> {
		validate_target_id(target_id)?;

		for key in hashing.json_key_to_hash_map.keys() {
			validate_key(key)?;
		}

		for hash in hashing.json_key_to_hash_map.values() {
			validate_hash(hash)?;
		}

		if let Some(parent) = self.file_path.parent() {
			if !parent.as_os_str().is_empty() {
				fs::create_dir_all(parent).await.map_err(|source| J18nError::Io {
					path: parent.to_path_buf(),
					source,
				})?;
			}
		}

		let temp_path = temp_path_for(&self.file_path);
		let serialized_section = serialize_section(target_id, hashing);

		if !fs::try_exists(&self.file_path).await.map_err(|source| J18nError::Io {
			path: self.file_path.clone(),
			source,
		})? {
			let mut bytes = serialized_section.into_bytes();

			bytes.push(b'\n');
			fs::write(&self.file_path, &bytes)
				.await
				.map_err(|source| J18nError::Io {
					path: self.file_path.clone(),
					source,
				})?;

			return Ok(());
		}

		let input = fs::File::open(&self.file_path).await.map_err(|source| J18nError::Io {
			path: self.file_path.clone(),
			source,
		})?;
		let output = fs::File::create(&temp_path).await.map_err(|source| J18nError::Io {
			path: temp_path.clone(),
			source,
		})?;
		let mut reader = BufReader::new(input).lines();
		let mut writer = BufWriter::new(output);
		let mut written_section = false;
		let mut skipping_old_section = false;
		let mut have_emitted_anything = false;

		while let Some(line) = reader.next_line().await.map_err(|source| J18nError::Io {
			path: self.file_path.clone(),
			source,
		})? {
			let trimmed = line.trim_end_matches('\r');

			if let Some(section_id) = parse_section_header(trimmed) {
				skipping_old_section = false;

				if section_id == target_id {
					if !written_section {
						write_section(&mut writer, &serialized_section, have_emitted_anything, &temp_path).await?;

						written_section = true;
						have_emitted_anything = true;
					}

					skipping_old_section = true;
					continue;
				}

				if !written_section && natural_key_cmp(target_id, section_id) == Ordering::Less {
					write_section(&mut writer, &serialized_section, have_emitted_anything, &temp_path).await?;

					written_section = true;
					have_emitted_anything = true;
				}
			}

			if skipping_old_section {
				continue;
			}

			if trimmed.is_empty() {
				continue;
			}

			if have_emitted_anything {
				writer.write_all(b"\n").await.map_err(|source| J18nError::Io {
					path: temp_path.clone(),
					source,
				})?;
			}

			writer
				.write_all(trimmed.as_bytes())
				.await
				.map_err(|source| J18nError::Io {
					path: temp_path.clone(),
					source,
				})?;
			have_emitted_anything = true;
		}

		if !written_section {
			write_section(&mut writer, &serialized_section, have_emitted_anything, &temp_path).await?;
		}

		writer.write_all(b"\n").await.map_err(|source| J18nError::Io {
			path: temp_path.clone(),
			source,
		})?;
		writer.flush().await.map_err(|source| J18nError::Io {
			path: temp_path.clone(),
			source,
		})?;

		fs::rename(&temp_path, &self.file_path)
			.await
			.map_err(|source| J18nError::Io {
				path: self.file_path.clone(),
				source,
			})?;

		Ok(())
	}
}

pub fn validate_target_id(target_id: &str) -> J18nResult<()> {
	if target_id.is_empty() {
		return Err(J18nError::validation("target id must not be empty"));
	}

	for character in target_id.chars() {
		if matches!(character, '[' | ']' | '\n' | '\r') {
			return Err(J18nError::validation(format!(
				"target id \"{target_id}\" contains an unsupported character ({character:?}); '[', ']', and newlines are not allowed"
			)));
		}
	}

	Ok(())
}

pub fn validate_key(key: &str) -> J18nResult<()> {
	if key.is_empty() {
		return Err(J18nError::validation("hash cache key must not be empty"));
	}

	for character in key.chars() {
		if matches!(character, '=' | '\n' | '\r') {
			return Err(J18nError::validation(format!(
				"hash cache key \"{key}\" contains an unsupported character ({character:?}); '=' and newlines are not allowed"
			)));
		}
	}

	Ok(())
}

fn validate_hash(hash: &str) -> J18nResult<()> {
	for character in hash.chars() {
		if matches!(character, '\n' | '\r') {
			return Err(J18nError::validation(format!(
				"hash value \"{hash}\" contains an unsupported character ({character:?}); newlines are not allowed"
			)));
		}
	}

	Ok(())
}

fn format_section_header(target_id: &str) -> String {
	format!("{SECTION_HEADER_OPEN}{target_id}{SECTION_HEADER_CLOSE}")
}

fn parse_section_header(line: &str) -> Option<&str> {
	let trimmed = line.trim();

	if trimmed.starts_with(SECTION_HEADER_OPEN) && trimmed.ends_with(SECTION_HEADER_CLOSE) && trimmed.len() >= 2 {
		Some(&trimmed[1..trimmed.len() - 1])
	} else {
		None
	}
}

fn serialize_section(target_id: &str, hashing: &I18nHashing) -> String {
	let mut sorted: Vec<(&String, &String)> = hashing.json_key_to_hash_map.iter().collect();

	sorted.sort_by(|a, b| natural_key_cmp(a.0, b.0));

	let mut buffer = String::new();

	buffer.push_str(&format_section_header(target_id));

	for (key, hash) in sorted {
		buffer.push('\n');
		buffer.push_str(key);
		buffer.push(KEY_VALUE_SEPARATOR);
		buffer.push_str(hash);
	}

	buffer
}

async fn write_section(
	writer: &mut BufWriter<fs::File>,
	serialized_section: &str,
	preceded_by_other_lines: bool,
	temp_path: &Path,
) -> J18nResult<()> {
	if preceded_by_other_lines {
		writer.write_all(b"\n").await.map_err(|source| J18nError::Io {
			path: temp_path.to_path_buf(),
			source,
		})?;
	}

	writer
		.write_all(serialized_section.as_bytes())
		.await
		.map_err(|source| J18nError::Io {
			path: temp_path.to_path_buf(),
			source,
		})?;

	Ok(())
}

fn temp_path_for(file_path: &Path) -> PathBuf {
	let file_name = file_path
		.file_name()
		.map(|name| name.to_string_lossy().into_owned())
		.unwrap_or_default();
	let parent = file_path.parent().map(Path::to_path_buf).unwrap_or_default();

	if parent.as_os_str().is_empty() {
		PathBuf::from(format!("{file_name}.tmp"))
	} else {
		parent.join(format!("{file_name}.tmp"))
	}
}

/// Builds a [`std::collections::BTreeMap`] view of every section currently
/// persisted in the store. Intended for tests and debugging: production code
/// paths should use [`I18nHashingStore::load`] for memory-bounded per-target
/// access.
#[cfg(test)]
pub(crate) async fn collect_all_sections(
	file_path: &Path,
) -> J18nResult<std::collections::BTreeMap<String, I18nHashing>> {
	if !fs::try_exists(file_path).await.map_err(|source| J18nError::Io {
		path: file_path.to_path_buf(),
		source,
	})? {
		return Ok(std::collections::BTreeMap::new());
	}

	let file = fs::File::open(file_path).await.map_err(|source| J18nError::Io {
		path: file_path.to_path_buf(),
		source,
	})?;
	let mut reader = BufReader::new(file).lines();
	let mut all = std::collections::BTreeMap::new();
	let mut current_id: Option<String> = None;
	let mut current = I18nHashing::empty();

	while let Some(line) = reader.next_line().await.map_err(|source| J18nError::Io {
		path: file_path.to_path_buf(),
		source,
	})? {
		let trimmed = line.trim_end_matches('\r');

		if let Some(section_id) = parse_section_header(trimmed) {
			if let Some(previous_id) = current_id.take() {
				all.insert(previous_id, std::mem::replace(&mut current, I18nHashing::empty()));
			}

			current_id = Some(section_id.to_string());

			continue;
		}

		if trimmed.is_empty() {
			continue;
		}

		if let Some((key, hash)) = trimmed.split_once(KEY_VALUE_SEPARATOR) {
			current.json_key_to_hash_map.insert(key.to_string(), hash.to_string());
		}
	}

	if let Some(previous_id) = current_id {
		all.insert(previous_id, current);
	}

	Ok(all)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::hashing::content_hash_hex;
	use std::collections::HashMap;
	use tempfile::TempDir;

	fn hashing_with(entries: &[(&str, &str)]) -> I18nHashing {
		let mut map = HashMap::new();

		for (key, value) in entries {
			map.insert((*key).to_string(), (*value).to_string());
		}

		I18nHashing {
			json_key_to_hash_map: map,
		}
	}

	#[tokio::test]
	async fn load_returns_empty_when_file_is_missing() {
		let dir = TempDir::new().unwrap();
		let store = I18nHashingStore::at(dir.path().join(".j18n-cache.ini"));

		let loaded = store.load("locales/pt.json@Portuguese").await.unwrap();

		assert!(loaded.json_key_to_hash_map.is_empty());
	}

	#[tokio::test]
	async fn load_returns_empty_when_section_for_target_is_missing() {
		let dir = TempDir::new().unwrap();
		let path = dir.path().join(".j18n-cache.ini");

		fs::write(&path, "[locales/pt.json@Portuguese]\na=1\n").await.unwrap();

		let store = I18nHashingStore::at(&path);
		let loaded = store.load("locales/es.json@Spanish").await.unwrap();

		assert!(loaded.json_key_to_hash_map.is_empty());
	}

	#[tokio::test]
	async fn save_then_load_round_trips_a_single_target() {
		let dir = TempDir::new().unwrap();
		let store = I18nHashingStore::at(dir.path().join(".j18n-cache.ini"));
		let hashing = hashing_with(&[("a", "1"), ("b", "2")]);

		store.save("locales/pt.json@Portuguese", &hashing).await.unwrap();

		let loaded = store.load("locales/pt.json@Portuguese").await.unwrap();

		assert_eq!(loaded.json_key_to_hash_map.get("a"), Some(&"1".to_string()));
		assert_eq!(loaded.json_key_to_hash_map.get("b"), Some(&"2".to_string()));
	}

	#[tokio::test]
	async fn save_creates_parent_directory_if_missing() {
		let dir = TempDir::new().unwrap();
		let path = dir.path().join("nested").join("deep").join(".j18n-cache.ini");
		let store = I18nHashingStore::at(&path);

		store.save("pt@Portuguese", &hashing_with(&[("a", "1")])).await.unwrap();

		assert!(path.is_file());
	}

	#[tokio::test]
	async fn save_writes_sections_sorted_by_target_id_with_keys_sorted_per_section() {
		let dir = TempDir::new().unwrap();
		let path = dir.path().join(".j18n-cache.ini");
		let store = I18nHashingStore::at(&path);

		store
			.save("z@Zulu", &hashing_with(&[("c", "3"), ("a", "1"), ("b", "2")]))
			.await
			.unwrap();
		store
			.save("a@Aymara", &hashing_with(&[("y", "y"), ("x", "x")]))
			.await
			.unwrap();

		let raw = fs::read_to_string(&path).await.unwrap();
		let expected = "[a@Aymara]\nx=x\ny=y\n[z@Zulu]\na=1\nb=2\nc=3\n";

		assert_eq!(raw, expected);
	}

	#[tokio::test]
	async fn save_replaces_only_the_given_target_section_preserving_others() {
		let dir = TempDir::new().unwrap();
		let path = dir.path().join(".j18n-cache.ini");
		let store = I18nHashingStore::at(&path);

		store.save("a@Aymara", &hashing_with(&[("x", "x")])).await.unwrap();
		store.save("z@Zulu", &hashing_with(&[("a", "OLD")])).await.unwrap();
		store
			.save("z@Zulu", &hashing_with(&[("a", "NEW"), ("b", "B")]))
			.await
			.unwrap();

		let aymara = store.load("a@Aymara").await.unwrap();
		let zulu = store.load("z@Zulu").await.unwrap();

		assert_eq!(aymara.json_key_to_hash_map.get("x"), Some(&"x".to_string()));
		assert_eq!(zulu.json_key_to_hash_map.get("a"), Some(&"NEW".to_string()));
		assert_eq!(zulu.json_key_to_hash_map.get("b"), Some(&"B".to_string()));
	}

	#[tokio::test]
	async fn save_inserts_new_section_in_sorted_position() {
		let dir = TempDir::new().unwrap();
		let path = dir.path().join(".j18n-cache.ini");
		let store = I18nHashingStore::at(&path);

		store.save("a@A", &hashing_with(&[("k", "1")])).await.unwrap();
		store.save("z@Z", &hashing_with(&[("k", "9")])).await.unwrap();
		store.save("m@M", &hashing_with(&[("k", "5")])).await.unwrap();

		let raw = fs::read_to_string(&path).await.unwrap();
		let a_position = raw.find("[a@A]").unwrap();
		let m_position = raw.find("[m@M]").unwrap();
		let z_position = raw.find("[z@Z]").unwrap();

		assert!(a_position < m_position);
		assert!(m_position < z_position);
	}

	#[tokio::test]
	async fn save_handles_empty_hashing_by_writing_only_the_section_header() {
		let dir = TempDir::new().unwrap();
		let path = dir.path().join(".j18n-cache.ini");
		let store = I18nHashingStore::at(&path);

		store.save("pt@Portuguese", &I18nHashing::empty()).await.unwrap();

		let raw = fs::read_to_string(&path).await.unwrap();

		assert_eq!(raw, "[pt@Portuguese]\n");
	}

	#[tokio::test]
	async fn load_short_circuits_after_passing_target_section() {
		let dir = TempDir::new().unwrap();
		let path = dir.path().join(".j18n-cache.ini");

		fs::write(
			&path,
			"[a@A]\nk=1\n[b@B]\nk=2\nbroken-line-without-equals\n[c@C]\nk=3\n",
		)
		.await
		.unwrap();

		let store = I18nHashingStore::at(&path);
		let a = store.load("a@A").await.unwrap();

		assert_eq!(a.json_key_to_hash_map.get("k"), Some(&"1".to_string()));
	}

	#[tokio::test]
	async fn load_errors_on_malformed_line_within_target_section() {
		let dir = TempDir::new().unwrap();
		let path = dir.path().join(".j18n-cache.ini");

		fs::write(&path, "[pt@Portuguese]\nbroken-line-without-equals\n")
			.await
			.unwrap();

		let store = I18nHashingStore::at(&path);

		assert!(store.load("pt@Portuguese").await.is_err());
	}

	#[tokio::test]
	async fn load_tolerates_blank_lines_between_sections() {
		let dir = TempDir::new().unwrap();
		let path = dir.path().join(".j18n-cache.ini");

		fs::write(&path, "[a@A]\nk=1\n\n[b@B]\nk=2\n").await.unwrap();

		let store = I18nHashingStore::at(&path);
		let a = store.load("a@A").await.unwrap();
		let b = store.load("b@B").await.unwrap();

		assert_eq!(a.json_key_to_hash_map.get("k"), Some(&"1".to_string()));
		assert_eq!(b.json_key_to_hash_map.get("k"), Some(&"2".to_string()));
	}

	#[tokio::test]
	async fn load_tolerates_crlf_line_endings() {
		let dir = TempDir::new().unwrap();
		let path = dir.path().join(".j18n-cache.ini");

		fs::write(&path, "[pt@Portuguese]\r\na=1\r\nb=2\r\n").await.unwrap();

		let store = I18nHashingStore::at(&path);
		let loaded = store.load("pt@Portuguese").await.unwrap();

		assert_eq!(loaded.json_key_to_hash_map.get("a"), Some(&"1".to_string()));
		assert_eq!(loaded.json_key_to_hash_map.get("b"), Some(&"2".to_string()));
	}

	#[tokio::test]
	async fn save_then_load_handles_negative_hex_hashes() {
		let dir = TempDir::new().unwrap();
		let store = I18nHashingStore::at(dir.path().join(".j18n-cache.ini"));
		let hash = content_hash_hex("Delete my account");

		store
			.save("pt@Portuguese", &hashing_with(&[("delete", &hash)]))
			.await
			.unwrap();

		let loaded = store.load("pt@Portuguese").await.unwrap();

		assert_eq!(loaded.json_key_to_hash_map.get("delete"), Some(&hash));
	}

	#[tokio::test]
	async fn save_rejects_target_id_with_brackets_or_newline() {
		let dir = TempDir::new().unwrap();
		let store = I18nHashingStore::at(dir.path().join(".j18n-cache.ini"));

		assert!(store.save("bad[id]", &I18nHashing::empty()).await.is_err());
		assert!(store.save("with\nnewline", &I18nHashing::empty()).await.is_err());
		assert!(store.save("", &I18nHashing::empty()).await.is_err());
	}

	#[tokio::test]
	async fn save_rejects_keys_with_equals_or_newline() {
		let dir = TempDir::new().unwrap();
		let store = I18nHashingStore::at(dir.path().join(".j18n-cache.ini"));

		assert!(store
			.save("pt@Portuguese", &hashing_with(&[("a=b", "1")]))
			.await
			.is_err());
		assert!(store
			.save("pt@Portuguese", &hashing_with(&[("a\nb", "1")]))
			.await
			.is_err());
	}

	#[test]
	fn validate_target_id_accepts_typical_ids() {
		assert!(validate_target_id("locales/pt.json@Portuguese").is_ok());
		assert!(validate_target_id("locales/pt/common.json@Brazilian Portuguese").is_ok());
	}

	#[test]
	fn validate_target_id_rejects_disallowed_characters() {
		assert!(validate_target_id("").is_err());
		assert!(validate_target_id("[bad]").is_err());
		assert!(validate_target_id("a\nb").is_err());
		assert!(validate_target_id("a\rb").is_err());
	}

	#[test]
	fn validate_key_accepts_dotted_paths() {
		assert!(validate_key("common.button.ok").is_ok());
		assert!(validate_key("auth-flow.login_title").is_ok());
	}

	#[test]
	fn validate_key_rejects_disallowed_characters() {
		assert!(validate_key("").is_err());
		assert!(validate_key("a=b").is_err());
		assert!(validate_key("a\nb").is_err());
	}

	#[test]
	fn parse_section_header_strips_brackets() {
		assert_eq!(parse_section_header("[a@A]"), Some("a@A"));
		assert_eq!(
			parse_section_header("[locales/pt.json@Portuguese]"),
			Some("locales/pt.json@Portuguese")
		);
	}

	#[test]
	fn parse_section_header_returns_none_for_non_section_lines() {
		assert_eq!(parse_section_header("a=1"), None);
		assert_eq!(parse_section_header(""), None);
		assert_eq!(parse_section_header("[unclosed"), None);
		assert_eq!(parse_section_header("unopened]"), None);
	}

	#[tokio::test]
	async fn collect_all_sections_returns_every_section() {
		let dir = TempDir::new().unwrap();
		let path = dir.path().join(".j18n-cache.ini");
		let store = I18nHashingStore::at(&path);

		store.save("a@A", &hashing_with(&[("k", "1")])).await.unwrap();
		store.save("b@B", &hashing_with(&[("k", "2")])).await.unwrap();

		let all = collect_all_sections(&path).await.unwrap();

		assert_eq!(all.len(), 2);
		assert_eq!(all["a@A"].json_key_to_hash_map.get("k"), Some(&"1".to_string()));
		assert_eq!(all["b@B"].json_key_to_hash_map.get("k"), Some(&"2".to_string()));
	}
}
