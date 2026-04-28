use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Debug, Default)]
pub struct I18nHashing {
	pub json_key_to_hash_map: HashMap<String, String>,
}

impl I18nHashing {
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

pub fn java_string_hashcode_hex(value: &str) -> String {
	let mut hash: i32 = 0;
	let mut buffer = [0u16; 2];

	for character in value.chars() {
		let units = character.encode_utf16(&mut buffer);

		for unit in units.iter() {
			hash = hash.wrapping_mul(31).wrapping_add(*unit as i32);
		}
	}

	format_signed_hex(hash)
}

fn format_signed_hex(value: i32) -> String {
	let widened = value as i64;

	if widened < 0 {
		format!("-{:x}", -widened)
	} else {
		format!("{:x}", widened)
	}
}
