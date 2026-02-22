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
echo "IMPORTANT: To enable cd and pushd/popd integration, add the following functions to your shell configuration (e.g., ~/.bashrc or ~/.zshrc):"
echo ""
echo "# Register current directory after successful directory changes."
echo "__goto_register_pwd() {"
echo "  command goto register \"\$PWD\" >/dev/null 2>&1 || true"
echo "}"
echo ""
echo "# Hook builtins so plain cd/pushd/popd also update goto history/index."
echo "cd() {"
echo "  builtin cd \"\$@\" || return"
echo "  __goto_register_pwd"
echo "}"
echo ""
echo "pushd() {"
echo "  builtin pushd \"\$@\" || return"
echo "  __goto_register_pwd"
echo "}"
echo ""
echo "popd() {"
echo "  builtin popd \"\$@\" || return"
echo "  __goto_register_pwd"
echo "}"
echo ""
echo "# cd integration"
echo "g() {"
echo "  local dir"
echo "  if [ \"\$#\" -eq 1 ] && [ \"\$1\" = \"-\" ]; then"
echo "    cd - || return"
echo "    return"
echo "  fi"
echo "  if [ \$# -eq 0 ]; then"
echo "    dir=\"\$(goto)\""
echo "  else"
echo "    dir=\"\$(goto \"\$@\" --auto 2>/dev/null)\" || dir=\"\$(goto \"\$@\")\""
echo "  fi"
echo "  if [ -n \"\$dir\" ]; then"
echo "    cd \"\$dir\""
echo "  fi"
echo "}"
echo ""
echo "# always-interactive cd"
echo "gi() {"
echo "  local dir"
echo "  dir=\"\$(goto \"\$@\")\""
echo "  if [ -n \"\$dir\" ]; then"
echo "    cd \"\$dir\""
echo "  fi"
echo "}"
echo ""
echo "Then, restart your terminal or run: source ~/.zshrc (or ~/.bashrc)"
echo "--------------------------------------------------"
