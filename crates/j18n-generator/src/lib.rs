use futures::stream::{self, StreamExt};
use j18n_core::{GenerationMode, I18nDefinition, J18nResult};
use j18n_io::{read_i18n_data, write_i18n_tree_map, I18nHashingCache};
use j18n_translator::I18nTranslator;
use j18n_validator::TranslationValidator;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::info;

const BATCH_SIZE: usize = 50;
const PARALLEL_LIMIT: usize = 3;
const HASH_CACHE_FILE_NAME: &str = ".hash-cache.json";

pub struct I18nGenerator;

impl I18nGenerator {
	pub async fn execute<T>(
		translator: &T,
		reference_i18n: &I18nDefinition,
		generate_i18n_for: &[I18nDefinition],
		mode: GenerationMode,
	) -> J18nResult<()>
	where
		T: I18nTranslator + ?Sized,
	{
		let reference_data = read_i18n_data(reference_i18n).await?;
		let reference_entries = reference_data.walked_tree_map.clone();
		let hash_cache_path = reference_i18n
			.json_file_path
			.parent()
			.map(|parent| parent.join(HASH_CACHE_FILE_NAME))
			.expect("reference json must have a parent directory");
		let reference_cached_hashing = I18nHashingCache::load_hash_cache_from(&hash_cache_path).await?;
		let reference_current_hashing = I18nHashingCache::compute_hash_cache_from(&reference_data);
		let changed_keys_since_last_hashing = reference_cached_hashing.compute_changed_keys(&reference_current_hashing);

		info!(
			"Scanned {} total entries from {} dict",
			reference_entries.len(),
			reference_i18n.language.language_name()
		);
		info!(
			"{} keys changed since last translation",
			changed_keys_since_last_hashing.len()
		);

		for target in generate_i18n_for {
			translate_into_target(
				translator,
				reference_i18n,
				&reference_data,
				&reference_entries,
				&changed_keys_since_last_hashing,
				target,
				mode,
			)
			.await?;
		}

		I18nHashingCache::save_hash_cache_to(&reference_current_hashing, &hash_cache_path).await?;

		Ok(())
	}
}

#[allow(clippy::too_many_arguments)]
async fn translate_into_target<T>(
	translator: &T,
	reference_i18n: &I18nDefinition,
	reference_data: &j18n_core::I18nData,
	reference_entries: &[(String, String)],
	changed_keys_since_last_hashing: &BTreeSet<String>,
	target: &I18nDefinition,
	mode: GenerationMode,
) -> J18nResult<()>
where
	T: I18nTranslator + ?Sized,
{
	let target_data = read_i18n_data(target).await?;
	let entries_to_translate: Vec<(String, String)> = match mode {
		GenerationMode::Regenerate => reference_entries.to_vec(),
		GenerationMode::Sync => {
			let target_keys: BTreeSet<&str> = target_data.walked_tree_map.iter().map(|(k, _)| k.as_str()).collect();

			reference_entries
				.iter()
				.filter(|(key, _)| !target_keys.contains(key.as_str()) || changed_keys_since_last_hashing.contains(key))
				.cloned()
				.collect()
		}
	};
	let total_characters: usize = entries_to_translate.iter().map(|(_, value)| value.len()).sum();
	let windowed_entries: Vec<Vec<(String, String)>> = entries_to_translate
		.chunks(BATCH_SIZE)
		.map(|chunk| chunk.to_vec())
		.collect();

	info!(
		"Translating {} entries ({} characters) to {} in a total of {} batches...",
		entries_to_translate.len(),
		total_characters,
		target.language.language_name(),
		windowed_entries.len()
	);

	let total_batches = windowed_entries.len();
	let translated_count = Arc::new(AtomicUsize::new(0));
	let translated_batches: Vec<J18nResult<Vec<(String, String)>>> =
		stream::iter(windowed_entries.into_iter().map(|window| {
			let translated_count = Arc::clone(&translated_count);

			async move {
				let result = translate_batch(translator, reference_i18n, target, window).await;
				let current = translated_count.fetch_add(1, Ordering::SeqCst) + 1;

				info!("Batch {current}/{total_batches} translated");

				result
			}
		}))
		.buffer_unordered(PARALLEL_LIMIT)
		.collect()
		.await;

	let translated_batches: Vec<Vec<(String, String)>> =
		translated_batches.into_iter().collect::<J18nResult<Vec<_>>>()?;

	info!("Writing ({mode}) JSON to \"{}\"...", target.json_file_path.display());

	let initial_json_dict: Map<String, Value> = match mode {
		GenerationMode::Regenerate => reference_data.json_dict.clone(),
		GenerationMode::Sync => merge_json_objects(&reference_data.json_dict, &target_data.json_dict),
	};

	write_i18n_tree_map(
		target,
		&reference_data.json_dict,
		initial_json_dict,
		&translated_batches,
	)
	.await?;

	Ok(())
}

async fn translate_batch<T>(
	translator: &T,
	from: &I18nDefinition,
	to: &I18nDefinition,
	batch: Vec<(String, String)>,
) -> J18nResult<Vec<(String, String)>>
where
	T: I18nTranslator + ?Sized,
{
	let mut batch_keys: Vec<String> = Vec::with_capacity(batch.len());
	let mut batch_values: Vec<String> = Vec::with_capacity(batch.len());

	for (key, value) in batch {
		batch_keys.push(key);
		batch_values.push(value);
	}

	let translated_values = translator
		.translate_i18n_values(from.language, to.language, batch_values.clone())
		.await?;

	TranslationValidator::validate_translation(&batch_values, &translated_values)?;

	let mut translations: Vec<(String, String)> = Vec::with_capacity(batch_keys.len());

	for (index, key) in batch_keys.into_iter().enumerate() {
		let translated_value = translated_values[index].clone();

		translations.push((key, translated_value));
	}

	Ok(translations)
}

fn merge_json_objects(first: &Map<String, Value>, second: &Map<String, Value>) -> Map<String, Value> {
	let mut merged = first.clone();

	for (key, value) in second {
		merged.insert(key.clone(), value.clone());
	}

	merged
}
