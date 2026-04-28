use j18n_core::{I18nData, I18nDefinition, J18nError, J18nResult};
use j18n_io::read_i18n_data;
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;
use tracing::{info, warn};

pub struct TranslationValidator;

impl TranslationValidator {
	pub async fn validate_translations(
		reference_i18n: &I18nDefinition,
		generated_i18ns: &[I18nDefinition],
	) -> J18nResult<()> {
		let reference_data = read_i18n_data(reference_i18n).await?;

		for generated in generated_i18ns {
			info!(
				"Validating {} ({}) with {} ({})...",
				generated.language.language_name(),
				generated.json_file_path.display(),
				reference_i18n.language.language_name(),
				reference_i18n.json_file_path.display()
			);

			let generated_data = read_i18n_data(generated).await?;

			Self::validate_data(&reference_data, &generated_data)?;
		}

		Ok(())
	}

	pub fn validate_data(reference_data: &I18nData, generated_data: &I18nData) -> J18nResult<()> {
		let generated_lookup: HashMap<&str, &str> = generated_data
			.walked_tree_map
			.iter()
			.map(|(key, value)| (key.as_str(), value.as_str()))
			.collect();

		for (key, reference_value) in &reference_data.walked_tree_map {
			let generated_value = generated_lookup
				.get(key.as_str())
				.ok_or_else(|| J18nError::MissingTranslation { key: key.clone() })?;

			check_interpolations(reference_value, generated_value);
		}

		Ok(())
	}

	pub fn validate_translation(reference_values: &[String], generated_values: &[String]) -> J18nResult<()> {
		if reference_values.len() != generated_values.len() {
			return Err(J18nError::validation(format!(
				"reference values size ({}) does not match generated values size ({})",
				reference_values.len(),
				generated_values.len()
			)));
		}

		for (reference, generated) in reference_values.iter().zip(generated_values.iter()) {
			check_interpolations(reference, generated);
		}

		Ok(())
	}
}

fn check_interpolations(reference: &str, generated: &str) {
	let reference_interpolations = find_interpolations(reference);
	let generated_interpolations = find_interpolations(generated);

	let same_count = reference_interpolations.len() == generated_interpolations.len();
	let contains_all = reference_interpolations
		.iter()
		.all(|i| generated_interpolations.contains(i));

	if !same_count || !contains_all {
		warn!("Wrong interpolations in \"{generated}\" (original: \"{reference}\")");
	}
}

fn find_interpolations(value: &str) -> Vec<String> {
	interpolations_regex()
		.find_iter(value)
		.map(|m| m.as_str().to_string())
		.collect()
}

fn interpolations_regex() -> &'static Regex {
	static INSTANCE: OnceLock<Regex> = OnceLock::new();

	INSTANCE.get_or_init(|| Regex::new(r"\{\{(.+?)\}\}").expect("valid interpolation regex"))
}
