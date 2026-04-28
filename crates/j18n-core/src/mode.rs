use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationMode {
	Regenerate,
	Sync,
}

impl fmt::Display for GenerationMode {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Regenerate => f.write_str("REGENERATE"),
			Self::Sync => f.write_str("SYNC"),
		}
	}
}
