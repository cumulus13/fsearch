# ⚡ fsearch — Fast File & Content Search

[![Crates.io](https://img.shields.io/crates/v/fast-search.svg)](https://crates.io/crates/fast-search)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![CI](https://github.com/cumulus13/fsearch/actions/workflows/ci.yml/badge.svg)](https://github.com/cumulus13/fsearch/actions)

A **blazingly fast**, production-ready, cross-platform CLI tool for searching
files and file contents — written in Rust.

---

## 🚀 Features

| Feature | Detail |
|---------|--------|
| ⚡ Two search engines | Method 1: parallel `walkdir` + `rayon`; Method 2: deterministic recursive |
| 📄 Content search | grep-style search inside files with line numbers |
| 🎨 True-colour output | Every colour is a configurable `#RRGGBB` hex value |
| 🔍 Glob patterns | `*` and `?` wildcards for file names and `-i` filters |
| 📁 Depth control | Limit recursion with `-d` |
| 🚫 Binary-safe | Auto-skips binary files; graceful permission handling |
| ⚙️ Config file | `~/.config/fsearch/config.toml` or `./fsearch.toml` |
| 🧵 Parallel search | Rayon thread pool (configurable thread count) |
| 🛑 Ctrl-C clean exit | Exits with code 130 |
| 🪟 Cross-platform | Linux · macOS · Windows |

---

## 📦 Installation

### From crates.io

```bash
cargo install fast-search
```

### Pre-built binaries

Download from [GitHub Releases](https://github.com/cumulus13/fsearch/releases):

| Platform | File |
|----------|------|
| Linux x86_64 (glibc) | `fsearch-linux-x86_64.tar.gz` |
| Linux aarch64 | `fsearch-linux-aarch64.tar.gz` |
| Linux x86_64 (musl/static) | `fsearch-linux-x86_64-musl.tar.gz` |
| macOS x86_64 | `fsearch-macos-x86_64.tar.gz` |
| macOS Apple Silicon | `fsearch-macos-aarch64.tar.gz` |
| Windows x86_64 | `fsearch-windows-x86_64.zip` |

### From source

```bash
git clone https://github.com/cumulus13/fsearch
cd fsearch
cargo build --release
# binary → ./target/release/fsearch  (or fsearch.exe on Windows)
```

---

## 🔧 Usage

```
fsearch [OPTIONS] [PATTERN]
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `PATTERN` | Pattern to search (`*` `?` wildcards supported) | — |
| `-m, --method <1\|2>` | Engine: 1 = walkdir+rayon, 2 = recursive | `1` |
| `-c, --case-insensitive` | Case-insensitive matching | on |
| `-C, --case-sensitive` | Case-sensitive matching | off |
| `-d, --deep <N>` | Maximum directory depth | `1` |
| `-p, --path <DIR>` | Search root directory | `.` |
| `-D, --no-dir` | Exclude directories from results | off |
| `-f, --file` | Search inside file contents | off |
| `-i, --include <GLOBS>` | Include only files matching patterns (CSV) | all |
| `-x, --exclude <DIRS>` | Extra directory names to skip (CSV) | — |
| `-n, --max-results <N>` | Limit results (0 = unlimited) | `0` |
| `-v, --verbose` | Print status, filters and elapsed time | off |
| `--init-config` | Write default config to `~/.config/fsearch/config.toml` | — |
| `--show-config` | Print active config as TOML and exit | — |

---

## 💡 Examples

```bash
# Find all Rust source files (depth 5)
fsearch '*.rs' -d 5

# Find files containing "TODO" in Python/JS/TS source
fsearch TODO -f -i '*.py,*.js,*.ts' -d 10

# Case-sensitive search for exact name
fsearch README -C

# Search in a specific directory, verbose output
fsearch config -p ~/projects -d 3 -v

# Exclude extra dirs on top of defaults
fsearch main -x dist,build -d 5

# Limit to first 20 results
fsearch '*.log' -n 20 -d 4

# Generate your config file
fsearch --init-config

# Inspect active config
fsearch --show-config
```

---

## ⚙️ Configuration

```bash
fsearch --init-config
# → ~/.config/fsearch/config.toml  (fully annotated)
```

Config lookup order (highest priority first):

1. `./fsearch.toml` — project-local override
2. `~/.config/fsearch/config.toml` — user config
3. Built-in defaults

### All config keys

```toml
# Search behaviour
default_depth       = 1       # default -d value
default_method      = 1       # 1 or 2
case_insensitive    = true
include_dirs        = true
binary_check_bytes  = 1024
max_line_length     = 10000
threads             = 0       # 0 = all logical CPUs

# Output
verbose             = false
show_size           = false
show_modified       = false
max_results         = 0       # 0 = unlimited

# Colours — any CSS hex string
color_index         = "#FF88FF"
color_path          = "#FFFF00"
color_line_num      = "#FF4444"
color_line_text     = "#00FFFF"
color_header        = "#FFFFFF"
color_count         = "#00FFFF"
color_error         = "#FF3333"
color_warn          = "#FFAA00"
color_info          = "#00FF88"
color_pattern       = "#FF00FF"

# Filters
exclude_dirs        = ".git,node_modules,.svn,__pycache__,.hg,target,.cache"
default_include     = ""      # e.g. "*.py,*.rs"
```

---

## 📊 Exit Codes

| Code | Meaning |
|------|---------|
| `0` | One or more matches found |
| `1` | No matches found, or error |
| `130` | Interrupted by Ctrl-C |

---

## 🏗️ Building for Windows from Linux

```bash
# Install rustup (https://rustup.rs), then:
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64

# Uncomment the [target.x86_64-pc-windows-gnu] block in .cargo/config.toml, then:
cargo build --release --target x86_64-pc-windows-gnu
# → target/x86_64-pc-windows-gnu/release/fsearch.exe
```

Or simply push a version tag — GitHub Actions builds all platforms automatically.

---

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/my-feature`)
3. Make your changes and ensure `cargo clippy -- -D warnings` passes
4. Submit a pull request

---

## 📄 License

MIT — see [LICENSE](LICENSE)

## 👤 Author

**Hadi Cahyadi** — [cumulus13@gmail.com](mailto:cumulus13@gmail.com)  
GitHub: [@cumulus13](https://github.com/cumulus13)

[![Buy Me a Coffee](https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png)](https://www.buymeacoffee.com/cumulus13)
[![Ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/cumulus13)
