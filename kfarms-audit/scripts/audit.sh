#!/bin/bash

# KFarms Protocol Security Audit Script
# This script orchestrates the complete audit pipeline

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
PROGRAM_PATH="${1:-./programs}"
PROGRAM_ID="${2:-11111111111111111111111111111111}"
OUTPUT_DIR="${3:-./audit_results}"
PARALLEL_JOBS="${4:-4}"

# Create output directory
mkdir -p "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR/logs"
mkdir -p "$OUTPUT_DIR/reports"
mkdir -p "$OUTPUT_DIR/artifacts"

# Logging
LOG_FILE="$OUTPUT_DIR/logs/audit_$(date +%Y%m%d_%H%M%S).log"
exec 1> >(tee -a "$LOG_FILE")
exec 2>&1

echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}           KFarms Protocol Security Audit v1.0                    ${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo ""

# Function to print phase headers
print_phase() {
    echo ""
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}  PHASE $1: $2${NC}"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

# Function to print status
print_status() {
    echo -e "${YELLOW}  ➜${NC} $1"
}

# Function to print success
print_success() {
    echo -e "${GREEN}  ✓${NC} $1"
}

# Function to print error
print_error() {
    echo -e "${RED}  ✗${NC} $1"
}

# Phase 1: Discovery and Scoping
print_phase "1" "Discovery and Scoping"

print_status "Collecting program artifacts..."
if [ -d "$PROGRAM_PATH" ]; then
    find "$PROGRAM_PATH" -name "*.rs" -type f | wc -l > "$OUTPUT_DIR/artifacts/file_count.txt"
    print_success "Found $(cat $OUTPUT_DIR/artifacts/file_count.txt) Rust files"
else
    print_error "Program path not found: $PROGRAM_PATH"
    exit 1
fi

print_status "Extracting Anchor IDL..."
if [ -f "$PROGRAM_PATH/target/idl/*.json" ]; then
    cp "$PROGRAM_PATH"/target/idl/*.json "$OUTPUT_DIR/artifacts/"
    print_success "IDL extracted"
else
    print_status "No IDL found, skipping..."
fi

print_status "Identifying critical instructions..."
grep -r "pub fn" "$PROGRAM_PATH" --include="*.rs" | \
    grep -E "(deposit|withdraw|claim|lock|update_emission|set_points)" | \
    cut -d: -f2 | sort -u > "$OUTPUT_DIR/artifacts/critical_instructions.txt"
print_success "Found $(wc -l < $OUTPUT_DIR/artifacts/critical_instructions.txt) critical instructions"

# Phase 2: Mathematical Specification
print_phase "2" "Mathematical Specification and Verification"

print_status "Building audit tool..."
cargo build --release 2>/dev/null
print_success "Build complete"

print_status "Running mathematical invariant checks..."
if cargo run --release -- math --state test_data/protocol_state.json > "$OUTPUT_DIR/logs/math_check.log" 2>&1; then
    print_success "Mathematical invariants verified"
else
    print_error "Mathematical invariant violations detected"
    cat "$OUTPUT_DIR/logs/math_check.log" | grep "violation" | head -5
fi

print_status "Running property-based tests..."
cargo test --release property_tests -- --nocapture > "$OUTPUT_DIR/logs/property_tests.log" 2>&1 &
PID_PROP=$!

print_status "Running conservation tests..."
cargo test --release conservation -- --nocapture > "$OUTPUT_DIR/logs/conservation_tests.log" 2>&1 &
PID_CONS=$!

wait $PID_PROP $PID_CONS
print_success "Property tests completed"

# Phase 3: Solana/Anchor Analysis
print_phase "3" "Solana/Anchor Security Analysis"

print_status "Analyzing account validation..."
cargo run --release -- solana --program "$PROGRAM_PATH" --id "$PROGRAM_ID" > "$OUTPUT_DIR/logs/solana_analysis.log" 2>&1
print_success "Account validation complete"

print_status "Checking PDA derivations..."
grep -r "find_program_address\|create_program_address" "$PROGRAM_PATH" --include="*.rs" > "$OUTPUT_DIR/artifacts/pda_usage.txt"
print_success "Found $(wc -l < $OUTPUT_DIR/artifacts/pda_usage.txt) PDA operations"

print_status "Analyzing token flows..."
grep -r "transfer\|mint_to\|burn" "$PROGRAM_PATH" --include="*.rs" | \
    grep -v "//" > "$OUTPUT_DIR/artifacts/token_operations.txt"
print_success "Found $(wc -l < $OUTPUT_DIR/artifacts/token_operations.txt) token operations"

# Phase 4: Threat Detection
print_phase "4" "Threat Pattern Detection"

print_status "Checking for arithmetic overflow patterns..."
grep -r "as u64\|as u32\|as u128" "$PROGRAM_PATH" --include="*.rs" | \
    grep -v "checked_" > "$OUTPUT_DIR/artifacts/unchecked_casts.txt" || true
UNCHECKED_COUNT=$(wc -l < "$OUTPUT_DIR/artifacts/unchecked_casts.txt")
if [ "$UNCHECKED_COUNT" -gt 0 ]; then
    print_error "Found $UNCHECKED_COUNT potentially unsafe type casts"
else
    print_success "No unsafe type casts found"
fi

print_status "Checking for division operations..."
grep -r "/" "$PROGRAM_PATH" --include="*.rs" | \
    grep -v "//" | grep -v "/\*" > "$OUTPUT_DIR/artifacts/divisions.txt" || true
print_success "Found $(wc -l < $OUTPUT_DIR/artifacts/divisions.txt) division operations to review"

print_status "Analyzing external points integration..."
if cargo run --release -- threats --history test_data/tx_history.json --window 100 > "$OUTPUT_DIR/logs/threat_detection.log" 2>&1; then
    print_success "No active threats detected"
else
    print_error "Potential threats identified"
fi

# Phase 5: Fuzzing (if available)
print_phase "5" "Fuzz Testing"

if command -v cargo-fuzz &> /dev/null; then
    print_status "Running fuzz tests (60 seconds)..."
    timeout 60 cargo +nightly fuzz run math_fuzz 2>/dev/null || true
    print_success "Fuzzing completed"
else
    print_status "cargo-fuzz not installed, skipping fuzz tests"
fi

# Phase 6: Report Generation
print_phase "6" "Report Generation"

print_status "Generating markdown report..."
cargo run --release -- report --output "$OUTPUT_DIR/reports" --format markdown
print_success "Markdown report generated"

print_status "Generating JSON report..."
cargo run --release -- report --output "$OUTPUT_DIR/reports" --format json
print_success "JSON report generated"

print_status "Generating HTML report..."
cargo run --release -- report --output "$OUTPUT_DIR/reports" --format html
print_success "HTML report generated"

# Summary
echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}                        AUDIT SUMMARY                             ${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"

# Count findings
CRITICAL_COUNT=$(grep -c "Critical" "$OUTPUT_DIR/reports/audit_report.json" 2>/dev/null || echo 0)
HIGH_COUNT=$(grep -c "High" "$OUTPUT_DIR/reports/audit_report.json" 2>/dev/null || echo 0)
MEDIUM_COUNT=$(grep -c "Medium" "$OUTPUT_DIR/reports/audit_report.json" 2>/dev/null || echo 0)
LOW_COUNT=$(grep -c "Low" "$OUTPUT_DIR/reports/audit_report.json" 2>/dev/null || echo 0)

echo ""
echo "  Findings:"
echo -e "    ${RED}Critical:${NC} $CRITICAL_COUNT"
echo -e "    ${RED}High:${NC}     $HIGH_COUNT"
echo -e "    ${YELLOW}Medium:${NC}   $MEDIUM_COUNT"
echo -e "    ${GREEN}Low:${NC}      $LOW_COUNT"
echo ""
echo "  Reports generated in: $OUTPUT_DIR/reports/"
echo "  Logs available in:    $OUTPUT_DIR/logs/"
echo "  Artifacts saved in:   $OUTPUT_DIR/artifacts/"
echo ""
echo -e "${GREEN}Audit completed successfully!${NC}"
echo ""