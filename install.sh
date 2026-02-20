#!/bin/bash

# Exit on any error
set -e

echo "Building and installing goto..."

# Check if cargo is installed
if ! command -v cargo &> /dev/null
then
    echo "Error: cargo could not be found. Please install Rust from https://rustup.rs/"
    exit 1
fi

# Build in release mode
cargo build --release

# Install the binary to ~/.cargo/bin
cargo install --path .

echo "--------------------------------------------------"
echo "Successfully installed 'goto'!"
echo "--------------------------------------------------"
echo "IMPORTANT: To enable the 'cd' functionality, add the following alias to your shell configuration (e.g., ~/.bashrc or ~/.zshrc):"
echo ""
echo 'g() {'
echo '  local dir'
echo '  dir="$(goto "$@")" && [ -n "$dir" ] && cd "$dir"'
echo '}'
echo ""
echo "Then, restart your terminal or run: source ~/.zshrc (or ~/.bashrc)"
echo "--------------------------------------------------"
