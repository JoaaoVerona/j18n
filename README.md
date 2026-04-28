# j18n

Rust CLI for generating and syncing localized i18n JSON files from a reference
language, using either Claude Code or the Gemini API as the translation
backend.

This is a Rust port of the `i18ngen` Kotlin tool that lived in
`skiley-api/tools/src/main/kotlin/net/skiley/api/tools/i18ngen`.

## Layout

The project is a Cargo workspace with one crate per concern:

| Crate              | Purpose                                                                         |
| ------------------ | ------------------------------------------------------------------------------- |
| `j18n-core`        | Shared types: `Language`, `I18nDefinition`, `I18nData`, `GenerationMode`, errors |
| `j18n-io`          | JSON reader/writer, key walker, hash-cache                                      |
| `j18n-translator`  | `I18nTranslator` trait and the placeholder extrapolation/restoration helpers    |
| `j18n-claude-code` | Translator that drives the local `claude` CLI as a subprocess                   |
| `j18n-gemini-api`  | Translator that calls the Gemini `generateContent` HTTP API                     |
| `j18n-validator`   | Sanity checks that translated values keep their `{{interpolations}}`            |
| `j18n-generator`   | Orchestrator: batches entries, runs translators, writes output, refreshes cache |
| `j18n-cli`         | Binary crate exposing the `j18n` executable                                     |

## CLI

```
j18n init       <PATH>
j18n sync       <CONFIG>...
j18n regenerate <CONFIG>...
```

- `init` – write a skeleton JSON configuration file at `<PATH>`. Refuses to
  overwrite an existing file. Creates parent directories as needed.
- `sync` – translate only entries that are missing in the target file or whose
  reference value changed since the last run (tracked via `.hash-cache.json`
  next to the reference file).
- `regenerate` – re-translate every entry in the reference, replacing the
  existing values. Equivalent to the old `REPLACE_ALL` mode.

Each positional `<CONFIG>` is a path to a JSON configuration file (see below);
the tool runs the chosen mode against each config in sequence. The translator
backend is selected per-config via the `translator` property.

## Configuration file schema

```json
{
    "baseDirectory": "path/to/locales",
    "referenceI18n": "en",
    "generateI18nFor": [
        "cs", "da", "de", "el", "es", "fi", "fil", "fr", "he", "hi",
        "id", "it", "ja", "ko", "ms", "nl", "pl", "pt", "ro", "ru",
        "sv", "tr", "uk", "zh-CN", "zh-TW"
    ],
    "translator": "claude-code"
}
```

- `baseDirectory` – directory containing the reference and generated language
  JSON files. A relative path is resolved against the directory of the config
  file, not the current working directory; absolute paths are used as-is.
- `referenceI18n` – ISO-639 code of the source language (e.g. `"en"`); the
  reference file is `<baseDirectory>/<referenceI18n>.json`.
- `generateI18nFor` – ISO-639 codes for the target languages; each is written
  to `<baseDirectory>/<code>.json`.
- `translator` – which backend to use. Either `"claude-code"` or
  `"gemini-api"`.

Compared to the original Kotlin tool: the `mode` property is gone (use the
`sync` / `regenerate` subcommand instead) and `translator` is new (it used to
be a positional CLI argument).

### Example

`api.sync-from-en.json`:

```json
{
    "baseDirectory": "i18n/src/main/resources/locales",
    "referenceI18n": "en",
    "generateI18nFor": [
        "cs", "da", "de", "el", "es", "fi", "fil", "fr", "he", "hi",
        "id", "it", "ja", "ko", "ms", "nl", "pl", "pt", "ro", "ru",
        "sv", "tr", "uk", "zh-CN", "zh-TW"
    ],
    "translator": "claude-code"
}
```

```
j18n sync api.sync-from-en.json web-landing.sync-from-en.json web-user.sync-from-en.json
```

To use the Gemini backend instead, set `"translator": "gemini-api"` in the
config and export `GEMINI_API_KEY`:

```
GEMINI_API_KEY=... j18n sync api.sync-from-en.json
```

Regenerate every translation from scratch:

```
j18n regenerate api.sync-from-en.json
```

## Backends

### `claude-code`

Spawns the local `claude` CLI (`cmd /C claude --model=opus -p` on Windows,
`claude --model=opus -p` elsewhere) and pipes the prompt through stdin. Make
sure the `claude` executable is on `PATH`.

### `gemini-api`

Calls the Gemini `generateContent` HTTP endpoint. Requires the `GEMINI_API_KEY`
environment variable; fails fast at startup if it is missing.

## Behavior parity with the Kotlin tool

- Reference file is `<baseDirectory>/<referenceI18n>.json`; targets are
  `<baseDirectory>/<code>.json`.
- `sample` keys at the root of the reference are stripped when read.
- The hash cache file is `.hash-cache.json` next to the reference. Hashes are
  computed using Java's `String.hashCode()` algorithm so existing
  `.hash-cache.json` files written by the Kotlin tool stay valid.
- Entries are translated in batches of 50, with up to 3 batches in flight at
  the same time.
- After writing each target file, keys absent from the reference file are
  pruned from the target.
- `{{name}}`-style interpolations are extracted to `[N]` placeholders before
  the LLM call and restored after.
- Output JSON is tab-indented with a trailing newline, matching the Kotlin
  output.

## Things deliberately not migrated

- `TranslationReplacement` (the `replacement/` package, including
  `PtTranslationReplacement`) is not ported.
- The `mode` property in configs has been replaced by the `sync` / `regenerate`
  subcommands.

## Building

```
cargo build --release -p j18n-cli
```

The binary is written to `target/release/j18n` (`j18n.exe` on Windows).

## Logging

Uses `tracing` with `tracing-subscriber`. Override the level via the
`RUST_LOG` env var, e.g. `RUST_LOG=debug j18n sync ...`. Logs are written to
stderr so stdout is free for piping.
