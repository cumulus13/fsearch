//! # fsearch
//!
//! A blazingly fast, cross-platform library for **file search** and
//! **duplicate detection**, supporting multiple root directories.
//!
//! ## File search — single directory
//! ```no_run
//! use fsearch::searcher::{fast_find, SearchOptions};
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! let opts = SearchOptions::builder("*.rs")
//!     .base_dir("./src")
//!     .max_depth(5)
//!     .build();
//!
//! let results = fast_find(&opts, Arc::new(AtomicBool::new(false))).unwrap();
//! for m in &results { println!("{}", m.path().display()); }
//! ```
//!
//! ## File search — multiple directories
//! ```no_run
//! use fsearch::searcher::{fast_find, SearchOptions};
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! let opts = SearchOptions::builder("TODO")
//!     .base_dirs(vec!["./src", "./tests", "./benches"])
//!     .search_in_files(true)
//!     .max_depth(10)
//!     .build();
//!
//! let results = fast_find(&opts, Arc::new(AtomicBool::new(false))).unwrap();
//! ```
//!
//! ## Duplicate detection — multiple directories
//! ```no_run
//! use fsearch::duplicates::{find_duplicates, DuplicateOptions, DuplicateMode};
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! // Detect cross-directory duplicates (e.g. local Photos vs NAS backup)
//! let opts = DuplicateOptions::builder(vec!["~/Photos", "/mnt/nas/Photos"])
//!     .max_depth(10)
//!     .mode(DuplicateMode::Content)
//!     .min_size(1024)
//!     .build();
//!
//! let (groups, summary) = find_duplicates(&opts, Arc::new(AtomicBool::new(false))).unwrap();
//! println!("{} groups, {} wasted", summary.groups_found, summary.wasted_human());
//! ```

pub mod binary;
pub mod colors;
pub mod config;
pub mod duplicates;
pub mod error;
pub mod output;
pub mod searcher;

// ── Convenience re-exports ────────────────────────────────────────────────────

pub use config::Config;
pub use duplicates::{
    find_duplicates, DuplicateGroup, DuplicateMode, DuplicateOptions, DuplicateSummary,
    HashAlgorithm,
};
pub use error::{FsearchError, FsearchResult};
pub use searcher::{fast_find, recursive_find, LineMatch, SearchMatch, SearchOptions};
