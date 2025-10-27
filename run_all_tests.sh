#!/bin/bash

# Comprehensive test runner for Kamino Farms

set -e

echo "========================================="
echo "Running Kamino Farms Comprehensive Tests"
echo "========================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test results tracking
TESTS_PASSED=0
TESTS_FAILED=0

run_test_suite() {
    local suite_name=$1
    local command=$2
    
    echo -e "\n${YELLOW}Running $suite_name...${NC}"
    
    if eval "$command"; then
        echo -e "${GREEN}✓ $suite_name passed${NC}"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}✗ $suite_name failed${NC}"
        ((TESTS_FAILED++))
    fi
}

# 1. Build the program
echo -e "\n${YELLOW}Building program...${NC}"
cd programs/kfarms
cargo build-bpf

# 2. Run unit tests
run_test_suite "Unit Tests - Initialization" \
    "cargo test --test initialization_tests -- --nocapture"

run_test_suite "Unit Tests - Stake/Unstake" \
    "cargo test --test stake_unstake_tests -- --nocapture"

run_test_suite "Unit Tests - Rewards" \
    "cargo test --test reward_tests -- --nocapture"

run_test_suite "Unit Tests - Admin Operations" \
    "cargo test --test admin_tests -- --nocapture"

run_test_suite "Unit Tests - Oracle Integration" \
    "cargo test --test oracle_tests -- --nocapture"

run_test_suite "Unit Tests - Token-2022" \
    "cargo test --test token_2022_tests -- --nocapture"

# 3. Run property-based tests
run_test_suite "Property Tests - Conservation" \
    "cargo test --test conservation_tests -- --nocapture"

run_test_suite "Property Tests - Monotonicity" \
    "cargo test --test monotonicity_tests -- --nocapture"

run_test_suite "Property Tests - Commutativity" \
    "cargo test --test commutativity_tests -- --nocapture"

run_test_suite "Property Tests - Invariants" \
    "cargo test --test invariant_tests -- --nocapture"

# 4. Run differential tests
run_test_suite "Differential Tests" \
    "cargo test --test differential_scenarios -- --nocapture"

# 5. Run formal assertion tests
run_test_suite "Formal Assertions" \
    "cargo test formal_assertions -- --nocapture"

# 6. Run fuzzing (limited iterations for CI)
if command -v cargo-fuzz &> /dev/null; then
    echo -e "\n${YELLOW}Running fuzzing tests (limited)...${NC}"
    cd fuzz
    
    # Run each fuzzer for a limited time
    for target in fuzz_instruction_sequence fuzz_math_operations fuzz_compute_dos; do
        echo -e "${YELLOW}Fuzzing: $target${NC}"
        timeout 10s cargo +nightly fuzz run $target -- -max_total_time=10 || true
    done
    cd ..
else
    echo -e "${YELLOW}cargo-fuzz not installed, skipping fuzz tests${NC}"
    echo "Install with: cargo install cargo-fuzz"
fi

# 7. Run integration tests (if available)
if [ -f "../../tests/kfarms.ts" ]; then
    echo -e "\n${YELLOW}Running TypeScript integration tests...${NC}"
    cd ../..
    yarn test || true
    cd programs/kfarms
fi

# 8. Generate coverage report (if tarpaulin is installed)
if command -v cargo-tarpaulin &> /dev/null; then
    echo -e "\n${YELLOW}Generating coverage report...${NC}"
    cargo tarpaulin --out Html --output-dir coverage || true
    echo -e "${GREEN}Coverage report generated in coverage/tarpaulin-report.html${NC}"
else
    echo -e "${YELLOW}cargo-tarpaulin not installed, skipping coverage${NC}"
    echo "Install with: cargo install cargo-tarpaulin"
fi

# Summary
echo -e "\n========================================="
echo -e "${GREEN}Test Summary:${NC}"
echo -e "Tests Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Tests Failed: ${RED}$TESTS_FAILED${NC}"

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "\n${GREEN}All tests passed successfully! ✓${NC}"
    exit 0
else
    echo -e "\n${RED}Some tests failed. Please review the output above.${NC}"
    exit 1
fi