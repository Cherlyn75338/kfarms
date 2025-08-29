#!/bin/bash

# Setup script for test environment

echo "Setting up test environment..."

# Install Rust if not present
if ! command -v rustc &> /dev/null; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# Install Solana CLI if not present
if ! command -v solana &> /dev/null; then
    echo "Installing Solana CLI..."
    sh -c "$(curl -sSfL https://release.solana.com/v1.17.18/install)"
    export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
fi

# Install Anchor if not present
if ! command -v anchor &> /dev/null; then
    echo "Installing Anchor..."
    cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
    avm install 0.29.0
    avm use 0.29.0
fi

# Install additional tools
echo "Installing additional tools..."
cargo install cargo-fuzz || true
cargo install cargo-tarpaulin || true

echo "Setup complete!"
echo "Please run: source $HOME/.cargo/env"