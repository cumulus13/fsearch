// File: src\binary.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description: 
// License: MIT

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Returns `true` when the file looks like binary content.
///
/// The heuristic reads the first `check_bytes` bytes of the file and checks:
/// * whether a null byte (`\0`) is present — classic binary signal
/// * whether the bytes fail UTF-8 decoding
///
/// Any IO error is treated conservatively as "binary" so the file is skipped.
pub fn is_binary(path: &Path, check_bytes: usize) -> bool {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return true,
    };

    let mut buf = vec![0u8; check_bytes];
    let n = match file.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return true,
    };

    let chunk = &buf[..n];

    // Null byte → almost certainly binary
    if chunk.contains(&0u8) {
        return true;
    }

    // Non-UTF-8 sequence → treat as binary
    std::str::from_utf8(chunk).is_err()
}
