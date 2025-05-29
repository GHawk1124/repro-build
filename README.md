# Repx: Reproducible Builds with Nix

A tool designed to provide bit-for-bit reproducible builds for Rust software using Nix inside Docker containers.

## Overview

Repx simplifies the complex process of creating reproducible development environments with Nix by templating the most common tools and cross-compilation setups. By leveraging Nix flakes, we can create hermetic build environments that are guaranteed to produce the same output regardless of the host system.

The tool automatically generates the necessary Nix flake files, manages Docker containers, and orchestrates the build process across multiple targets. This allows developers to easily create cross-platform builds without deep knowledge of Nix or Docker.

## Key Features

- **Cross-compilation** for multiple targets (Linux x86_64/ARM64, Windows MSVC/GNU)
- **Static linking** options for minimal dependencies
- **Custom Rust channel** selection (stable, nightly)
- **Specific Rust version** support
- **Additional Nix inputs** management
- **Rich progress reporting** during builds
- **Detailed logging** with all commands and output
- **Target listing** with `--list-targets` flag

## Core Philosophy

Repx achieves the highest degree of reproducibility by controlling every aspect of the build environment:

- Precise Docker image version for Nix
- Exact Rust toolchain version
- Defined compiler flags and linking options
- Controlled dependency resolution

## Installation

```bash
cargo install repx
```

## Quick Start

```bash
# List available targets
repx --list-targets

# Build for current host platform
repx

# Build for specific targets
repx --target x86_64-unknown-linux-gnu --target aarch64-unknown-linux-gnu

# Build with static linking
repx --static

# Use specific Rust channel
repx --channel nightly
```

## Use as Library

Repx can also be integrated into your build process via `build.rs`:

```rust
// In your build.rs
use repx_lib::build_integration;

fn main() {
    build_integration::setup_reproducible_build().unwrap();
}
```

## Requirements

- Docker
- Internet connection (for downloading Nix and dependencies)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request or open an Issue.

## License

MIT License - see LICENSE file for details.