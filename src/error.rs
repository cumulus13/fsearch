// File: src\error.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description:
// License: MIT

//! Error types for the `fsearch` library.

use thiserror::Error;

/// All errors that can be produced by the library.
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum FsearchError {
    // ── Search errors ─────────────────────────────────────────────────────────
    #[error("🚫 Directory does not exist: {0}")]
    DirectoryNotFound(String),

    #[error("🚫 Path is not a directory: {0}")]
    NotADirectory(String),

    #[error("❌ Invalid depth: must be >= 0, got {0}")]
    InvalidDepth(i64),

    #[error("❌ Invalid pattern '{pattern}': {reason}")]
    InvalidPattern { pattern: String, reason: String },

    // ── Duplicate errors ──────────────────────────────────────────────────────
    #[error("🔍 Hash algorithm '{0}' is not supported (use 'md5' or 'sha256')")]
    UnsupportedHashAlgorithm(String),

    // ── IO errors ─────────────────────────────────────────────────────────────
    #[error("📁 IO error on '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("🔒 Permission denied: {0}")]
    PermissionDenied(String),

    // ── Config errors ─────────────────────────────────────────────────────────
    #[error("⚙️  Config error: {0}")]
    Config(String),

    #[error("⚙️  Config parse error in '{path}': {source}")]
    ConfigParse {
        path: String,
        #[source]
        source: toml::de::Error,
    },

    // ── Interrupt ─────────────────────────────────────────────────────────────
    #[error("🛑 Operation interrupted by user")]
    Interrupted,
}

/// Convenience alias used throughout the library.
pub type FsearchResult<T> = Result<T, FsearchError>;
