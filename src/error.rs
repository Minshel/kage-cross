use std::fmt;
use std::io;
use std::path::Path;

#[derive(Debug)]
pub enum KageError {
    Parse {
        file: String,
        line: u32,
        col: u32,
        msg: String,
    },
    Build {
        msg: String,
    },
    Io {
        path: Option<String>,
        source: io::Error,
    },
}

pub type Result<T> = std::result::Result<T, KageError>;

impl KageError {
    pub fn parse(file: &str, line: u32, col: u32, msg: impl Into<String>) -> Self {
        Self::Parse {
            file: file.to_string(),
            line,
            col,
            msg: msg.into(),
        }
    }

    pub fn build(msg: impl Into<String>) -> Self {
        Self::Build { msg: msg.into() }
    }

    pub fn io(path: impl AsRef<Path>, source: io::Error) -> Self {
        Self::Io {
            path: Some(path.as_ref().display().to_string()),
            source,
        }
    }
}

impl fmt::Display for KageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse {
                file,
                line,
                col,
                msg,
            } => write!(f, "{file}:{line}:{col}: {msg}"),
            Self::Build { msg } => write!(f, "kage: {msg}"),
            Self::Io {
                path: Some(p),
                source,
            } => write!(f, "kage: {p}: {source}"),
            Self::Io { path: None, source } => write!(f, "kage: {source}"),
        }
    }
}

impl std::error::Error for KageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for KageError {
    fn from(source: io::Error) -> Self {
        Self::Io { path: None, source }
    }
}
