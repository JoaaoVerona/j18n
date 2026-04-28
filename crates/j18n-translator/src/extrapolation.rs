use j18n_core::{J18nError, J18nResult};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub struct ExtrapolatedValue {
	pub extrapolated_value: String,
	pub interpolations_index_based: Vec<String>,
	pub original_value: String,
}

pub fn create_extrapolated_value(value: &str) -> ExtrapolatedValue {
	let interpolations_regex = interpolations_regex();
	let mut interpolations: Vec<String> = Vec::new();
	let mut extrapolated = value.to_string();

	loop {
		let Some(captures) = interpolations_regex.find(&extrapolated) else {
			break;
		};
		let placeholder = format!("[{}]", interpolations.len());
		let captured_value = captures.as_str().to_string();

		extrapolated = extrapolated.replacen(&captured_value, &placeholder, 1);
		interpolations.push(captured_value);
	}

	ExtrapolatedValue {
		extrapolated_value: extrapolated,
		interpolations_index_based: interpolations,
		original_value: value.to_string(),
	}
}

pub fn create_extrapolated_values(values: &[String]) -> Vec<ExtrapolatedValue> {
	values.iter().map(|v| create_extrapolated_value(v)).collect()
}

pub fn restore_extrapolated_value(extrapolated: &ExtrapolatedValue, translated_value: &str) -> J18nResult<String> {
	let substitutions_regex = substitutions_regex();
	let interpolations = &extrapolated.interpolations_index_based;
	let mut current = translated_value.to_string();
	let mut restored = 0usize;

	loop {
		let Some(captures) = substitutions_regex.captures(&current) else {
			break;
		};
		let whole_match = captures.get(0).expect("regex group 0 must exist").as_str().to_string();
		let index_str = captures.get(1).expect("regex group 1 must exist").as_str();
		let index: usize = index_str.parse().map_err(|_| {
			J18nError::translator(format!(
				"failed to parse placeholder index in {whole_match} (translated value: {translated_value})"
			))
		})?;

		if index >= interpolations.len() {
			return Err(J18nError::translator(format!(
				"failed to restore extrapolated value after translation\n\
				did not find interpolation substitution for placeholder:\n\
				original       = \"{}\"\n\
				sent (extrap.) = \"{}\"\n\
				translated     = \"{}\"\n\
				currently      = \"{current}\"\n\
				missing index  = {index}\n\
				interpolations = [{}]",
				extrapolated.original_value,
				extrapolated.extrapolated_value,
				translated_value,
				interpolations.join(",")
			)));
		}

		let interpolation = &interpolations[index];

		current = current.replacen(&whole_match, interpolation, 1);
		restored += 1;
	}

	if restored != interpolations.len() {
		return Err(J18nError::translator(format!(
			"failed to restore extrapolated value after translation\n\
			interpolated value does not have all interpolations restored:\n\
			original       = \"{}\"\n\
			sent (extrap.) = \"{}\"\n\
			translated     = \"{translated_value}\"\n\
			currently      = \"{current}\"\n\
			restored       = {restored}\n\
			expected       = {}\n\
			interpolations = [{}]",
			extrapolated.original_value,
			extrapolated.extrapolated_value,
			interpolations.len(),
			interpolations.join(",")
		)));
	}

	Ok(current)
}

pub fn restore_extrapolated_values(
	extrapolated_values: &[ExtrapolatedValue],
	translated_values: &[String],
) -> J18nResult<Vec<String>> {
	if translated_values.len() != extrapolated_values.len() {
		return Err(J18nError::translator(format!(
			"translation returned {} values but expected {}",
			translated_values.len(),
			extrapolated_values.len()
		)));
	}

	let mut output = Vec::with_capacity(translated_values.len());

	for (translated, extrapolated) in translated_values.iter().zip(extrapolated_values.iter()) {
		output.push(restore_extrapolated_value(extrapolated, translated)?);
	}

	Ok(output)
}

fn interpolations_regex() -> &'static Regex {
	static INSTANCE: OnceLock<Regex> = OnceLock::new();

	INSTANCE.get_or_init(|| Regex::new(r"\{\{(.+?)\}\}").expect("valid interpolation regex"))
}

fn substitutions_regex() -> &'static Regex {
	static INSTANCE: OnceLock<Regex> = OnceLock::new();

	INSTANCE.get_or_init(|| Regex::new(r"\[(\d+?)\]").expect("valid substitution regex"))
}
