// File: src\error.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description: 
// License: MIT

use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)] // variants are part of the public API; not all are constructed yet
pub enum SearchError {
    #[error("🚫 Directory does not exist: {0}")]
    DirectoryNotFound(String),

    #[error("🚫 Path is not a directory: {0}")]
    NotADirectory(String),

    #[error("❌ Invalid depth value: {0}")]
    InvalidDepth(String),

    #[error("❌ Invalid regex pattern '{pattern}': {reason}")]
    InvalidRegex { pattern: String, reason: String },

    #[error("📁 IO error accessing '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("🔒 Permission denied: {0}")]
    PermissionDenied(String),

    #[error("⚙️  Config error: {0}")]
    Config(String),

    #[error("🛑 Search interrupted by user")]
    Interrupted,
}

pub type SearchResult<T> = Result<T, SearchError>;
