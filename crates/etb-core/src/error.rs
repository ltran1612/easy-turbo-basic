use std::path::PathBuf;

/// A named substitution in a problem message.
///
/// `Key` exists because some substitutions are themselves prose. "This is a
/// {what}" needs `{what}` in the user's language too — filling it with an English noun
/// phrase produced "Đây là PDF document, không phải mã nguồn BASIC", an English
/// fragment stranded in a Vietnamese sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProblemArg {
    /// Already in the user's terms: a file name, or a proper name like "64-bit x86".
    Text(String),
    /// An i18n key to resolve in the user's language before substituting.
    Key(&'static str),
}

impl ProblemArg {
    pub fn text(s: impl Into<String>) -> Self {
        ProblemArg::Text(s.into())
    }
}

/// Why one of the user's files cannot be used, in a form the interface can translate.
///
/// One type for every such refusal — a "source" that is not source, a program saved
/// in a binary form. The shape of the answer is always the same: name the file, name the
/// problem, and let the interface say it in the user's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProblem {
    /// The file name as the user sees it.
    pub name: String,
    /// i18n key for the heading, naming which list the file is in.
    pub title_key: &'static str,
    /// i18n key for the explanation.
    pub reason_key: &'static str,
    /// Named substitutions for `reason_key`.
    pub args: Vec<(&'static str, ProblemArg)>,
}

#[derive(Debug, thiserror::Error)]
pub enum EtbError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("refused to write outside the application data directory: {path}")]
    EscapesWriteRoot { path: PathBuf },

    #[error("source file name is not usable: {reason}")]
    UnsafeSourceName { reason: String },

    #[error("no QB64 Phoenix Edition compiler (fbc) could be found")]
    ToolchainMissing,

    #[error("toolchain at {path} failed verification: {reason}")]
    ToolchainInvalid { path: PathBuf, reason: String },

    #[error("could not determine the platform directories for this user")]
    NoProjectDirs,

    /// Boxed because `toml::de::Error` alone is 96 bytes, which made `EtbError`
    /// — and therefore every `Result` in the workspace — larger than clippy's
    /// `result_large_err` threshold on Windows.
    #[error("configuration file {path} could not be parsed: {source}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },

    #[error("configuration file {path} was written by a newer version (schema {found}, we support {supported})")]
    ConfigTooNew {
        path: PathBuf,
        found: u32,
        supported: u32,
    },

    #[error("configuration could not be serialized: {0}")]
    ConfigSerialize(#[from] toml::ser::Error),

    #[error("source file is too large ({len} bytes, limit {limit})")]
    SourceTooLarge { len: u64, limit: u64 },

    #[error("the program has no source files")]
    NoSources,

    /// One of the user's files cannot be used, and we can say why in their own language.
    ///
    /// Boxed: `FileProblem` is several strings wide, and every `Result` in the
    /// workspace would otherwise carry that width. `english` is the same fact in a
    /// technical register, for a log or a support transcript — deliberately not the
    /// same sentence the interface shows the user.
    #[error("`{}`: {english}", problem.name)]
    UnusableFile {
        problem: Box<FileProblem>,
        english: String,
    },

    #[error("{0}")]
    Other(String),
}

impl EtbError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        EtbError::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T, E = EtbError> = std::result::Result<T, E>;
