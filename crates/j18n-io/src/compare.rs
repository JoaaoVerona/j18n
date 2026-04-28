use std::cmp::Ordering;

pub fn natural_key_cmp(a: &str, b: &str) -> Ordering {
	a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn lowercase_after_camel_case_with_shared_prefix_orders_lowercase_first() {
		assert_eq!(natural_key_cmp("none", "noSuggestions"), Ordering::Less);
		assert_eq!(natural_key_cmp("noSuggestions", "none"), Ordering::Greater);
	}

	#[test]
	fn returns_equal_for_identical_strings() {
		assert_eq!(natural_key_cmp("abc", "abc"), Ordering::Equal);
	}

	#[test]
	fn falls_back_to_case_sensitive_byte_order_when_lowercased_match() {
		assert_eq!(natural_key_cmp("Foo", "foo"), Ordering::Less);
		assert_eq!(natural_key_cmp("foo", "Foo"), Ordering::Greater);
	}

	#[test]
	fn sorts_a_realistic_set_of_camel_case_and_lowercase_keys_naturally() {
		let mut keys = vec!["noSuggestions", "none", "noResults", "noteCount", "notes"];

		keys.sort_by(|a, b| natural_key_cmp(a, b));

		assert_eq!(keys, vec!["none", "noResults", "noSuggestions", "noteCount", "notes"]);
	}

	#[test]
	fn falls_back_to_case_sensitive_byte_order_for_non_ascii_letters() {
		assert_eq!(natural_key_cmp("über", "Über"), Ordering::Greater);
		assert_eq!(natural_key_cmp("Über", "über"), Ordering::Less);
	}
}
