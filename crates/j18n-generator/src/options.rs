use j18n_core::PathPattern;
use regex::Regex;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct J18nOptions {
	pub batch_size: usize,
	pub exclude_patterns: Vec<PathPattern>,
	pub hash_cache_path: PathBuf,
	pub interpolation_patterns: Vec<Regex>,
	pub parallel_batches: usize,
}

impl J18nOptions {
	pub fn validate(&self) -> Result<(), String> {
		if self.batch_size == 0 {
			return Err("batchSize must be at least 1".to_string());
		}

		if self.parallel_batches == 0 {
			return Err("parallelBatches must be at least 1".to_string());
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn options_with(batch_size: usize, parallel_batches: usize) -> J18nOptions {
		J18nOptions {
			batch_size,
			exclude_patterns: vec![],
			hash_cache_path: PathBuf::from(".hash-cache.json"),
			interpolation_patterns: vec![],
			parallel_batches,
		}
	}

	#[test]
	fn validate_accepts_positive_values() {
		assert!(options_with(50, 3).validate().is_ok());
		assert!(options_with(1, 1).validate().is_ok());
	}

	#[test]
	fn validate_rejects_zero_batch_size() {
		let err = options_with(0, 3).validate().unwrap_err();

		assert!(err.contains("batchSize"));
	}

	#[test]
	fn validate_rejects_zero_parallel_batches() {
		let err = options_with(50, 0).validate().unwrap_err();

		assert!(err.contains("parallelBatches"));
	}
}
