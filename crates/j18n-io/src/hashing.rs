use j18n_core::I18nData;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Debug, Default)]
pub struct I18nHashing {
	pub json_key_to_hash_map: HashMap<String, String>,
}

impl I18nHashing {
	pub fn empty() -> Self {
		Self::default()
	}

	pub fn from_i18n_data(data: &I18nData) -> Self {
		let json_key_to_hash_map = data
			.walked_tree_map
			.iter()
			.map(|(key, value)| (key.clone(), content_hash_hex(value)))
			.collect();

		Self { json_key_to_hash_map }
	}

	pub fn compute_changed_keys(&self, compare_with: &I18nHashing) -> BTreeSet<String> {
		let mut changed_keys = BTreeSet::new();
		let mut all_keys = BTreeSet::new();

		all_keys.extend(self.json_key_to_hash_map.keys().cloned());
		all_keys.extend(compare_with.json_key_to_hash_map.keys().cloned());

		for key in all_keys {
			let reference_value = self.json_key_to_hash_map.get(&key);
			let target_value = compare_with.json_key_to_hash_map.get(&key);

			if reference_value != target_value {
				changed_keys.insert(key);
			}
		}

		changed_keys
	}
}

const FNV_64_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_64_PRIME: u64 = 0x100000001b3;

/// Deterministic 64-bit content hash of `value`'s UTF-8 bytes, formatted as a
/// fixed-width 16-character lowercase hex string. Uses FNV-1a — std-only, no
/// external dependencies, identical output across platforms and Rust
/// versions.
pub fn content_hash_hex(value: &str) -> String {
	let mut hash: u64 = FNV_64_OFFSET_BASIS;

	for byte in value.as_bytes() {
		hash ^= *byte as u64;
		hash = hash.wrapping_mul(FNV_64_PRIME);
	}

	format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn empty_string_hashes_to_fnv_offset_basis() {
		assert_eq!(content_hash_hex(""), "cbf29ce484222325");
	}

	#[test]
	fn output_is_fixed_width_sixteen_lowercase_hex_chars() {
		for input in ["a", "hello", "a much longer sentence with punctuation, etc."] {
			let hash = content_hash_hex(input);

			assert_eq!(hash.len(), 16, "input {input:?} produced {hash:?}");
			assert!(
				hash.chars().all(|character| matches!(character, '0'..='9' | 'a'..='f')),
				"input {input:?} produced non-hex {hash:?}",
			);
		}
	}

	#[test]
	fn deterministic_for_same_input() {
		assert_eq!(content_hash_hex("hello"), content_hash_hex("hello"));
		assert_eq!(content_hash_hex("héllo"), content_hash_hex("héllo"));
		assert_eq!(content_hash_hex(""), content_hash_hex(""));
	}

	#[test]
	fn distinguishes_short_strings_that_collide_in_java_hashcode() {
		assert_ne!(content_hash_hex("Aa"), content_hash_hex("BB"));
		assert_ne!(content_hash_hex("FB"), content_hash_hex("Ea"));
	}

	#[test]
	fn distinguishes_real_translation_strings() {
		assert_ne!(
			content_hash_hex("This account no longer exists."),
			content_hash_hex("Delete my account"),
		);
		assert_ne!(content_hash_hex("Hello"), content_hash_hex("Hi"));
	}

	#[test]
	fn handles_unicode_via_utf8_bytes() {
		let composed = content_hash_hex("héllo");
		let ascii = content_hash_hex("hello");

		assert_ne!(composed, ascii);
		assert_eq!(composed, content_hash_hex("héllo"));
	}

	#[test]
	fn from_i18n_data_uses_content_hash_for_each_value() {
		let data = I18nData {
			json_dict: Default::default(),
			walked_tree_map: vec![("greeting".into(), "abc".into())],
		};
		let hashing = I18nHashing::from_i18n_data(&data);

		assert_eq!(hashing.json_key_to_hash_map.get("greeting").unwrap(), &content_hash_hex("abc"));
	}

	#[test]
	fn compute_changed_keys_finds_added_removed_and_modified() {
		let mut a = HashMap::new();

		a.insert("kept_same".to_string(), "abc".to_string());
		a.insert("modified".to_string(), "old".to_string());
		a.insert("only_in_a".to_string(), "x".to_string());

		let mut b = HashMap::new();

		b.insert("kept_same".to_string(), "abc".to_string());
		b.insert("modified".to_string(), "new".to_string());
		b.insert("only_in_b".to_string(), "y".to_string());

		let hashing_a = I18nHashing {
			json_key_to_hash_map: a,
		};
		let hashing_b = I18nHashing {
			json_key_to_hash_map: b,
		};
		let changed = hashing_a.compute_changed_keys(&hashing_b);

		assert!(changed.contains("modified"));
		assert!(changed.contains("only_in_a"));
		assert!(changed.contains("only_in_b"));
		assert!(!changed.contains("kept_same"));
	}

	#[test]
	fn compute_changed_keys_returns_empty_when_identical() {
		let mut map = HashMap::new();

		map.insert("a".to_string(), "1".to_string());

		let hashing_a = I18nHashing {
			json_key_to_hash_map: map.clone(),
		};
		let hashing_b = I18nHashing {
			json_key_to_hash_map: map,
		};

		assert!(hashing_a.compute_changed_keys(&hashing_b).is_empty());
	}
}
