# Installation

## Requirements

`pwer` requires macOS for live collection.

Building from source requires Rust 1.88 or newer and the macOS system frameworks supplied by Xcode Command Line Tools.

## Install from crates.io

```bash
cargo install pwer
```

## Install from source

```bash
git clone https://github.com/narumiruna/pwer.git
cd pwer
cargo install --path .
```

## Verify

```bash
pwer --version
pwer --help
```

No daemon, kernel extension, or root installation is required.
