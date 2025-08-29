#!/bin/bash

# Kamino Farms Security Audit Test Runner
# This script runs all the security and mathematical tests created during the audit

echo "========================================="
echo "Kamino Farms Security Audit Test Suite"
echo "========================================="
echo ""

# Navigate to the farms program directory
cd programs/kfarms

# Run all test modules
echo "Running Math Tests..."
cargo test --lib tests::math_tests -- --nocapture

echo ""
echo "Running Withdrawal Penalty Tests..."
cargo test --lib tests::withdrawal_penalty_tests -- --nocapture

echo ""
echo "Running Overflow Safety Tests..."
cargo test --lib tests::overflow_safety_tests -- --nocapture

echo ""
echo "Running Stake Operations Tests..."
cargo test --lib tests::stake_operations_tests -- --nocapture

echo ""
echo "Running Precision Tests..."
cargo test --lib tests::precision_tests -- --nocapture

echo ""
echo "Running Invariant Tests..."
cargo test --lib tests::invariant_tests -- --nocapture

echo ""
echo "Running Fuzz Tests..."
cargo test --lib tests::fuzz_tests -- --nocapture

echo ""
echo "Running Farm Operations Tests..."
cargo test --lib tests::farm_operations_tests -- --nocapture

echo ""
echo "========================================="
echo "Test Suite Complete"
echo "========================================="
echo ""
echo "For detailed results, check the test output above."
echo "Critical issues found are documented in AUDIT_REPORT.md"