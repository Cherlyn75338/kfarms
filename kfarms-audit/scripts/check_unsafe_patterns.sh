#!/bin/bash

# Check for unsafe patterns in Rust code

PROGRAM_PATH="${1:-.}"
OUTPUT_FILE="${2:-unsafe_patterns.txt}"

echo "Scanning for unsafe patterns in $PROGRAM_PATH..."
echo "Results will be saved to $OUTPUT_FILE"
echo ""

# Clear output file
> "$OUTPUT_FILE"

# Function to check pattern and report
check_pattern() {
    local pattern="$1"
    local description="$2"
    local severity="$3"
    
    echo "Checking: $description"
    echo "[$severity] $description" >> "$OUTPUT_FILE"
    echo "=" >> "$OUTPUT_FILE"
    
    if rg "$pattern" "$PROGRAM_PATH" -g "*.rs" --no-heading >> "$OUTPUT_FILE" 2>/dev/null; then
        count=$(rg "$pattern" "$PROGRAM_PATH" -g "*.rs" -c | awk -F: '{sum+=$2} END {print sum}')
        echo "  Found $count instances"
        echo "" >> "$OUTPUT_FILE"
    else
        echo "  None found ✓"
        echo "None found" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
    fi
}

# Critical patterns
echo "=== CRITICAL PATTERNS ===" >> "$OUTPUT_FILE"
check_pattern "unsafe\s*{" "Unsafe blocks" "CRITICAL"
check_pattern "std::mem::transmute" "Transmute usage" "CRITICAL"
check_pattern "std::ptr::" "Raw pointer operations" "CRITICAL"

# High severity patterns
echo "=== HIGH SEVERITY PATTERNS ===" >> "$OUTPUT_FILE"
check_pattern "\bas\s+u(32|64|128)" "Unchecked type casting" "HIGH"
check_pattern "unwrap\(\)" "Unwrap without error handling" "HIGH"
check_pattern "expect\(" "Expect that can panic" "HIGH"
check_pattern "/\s*[^/\*]" "Division without zero check" "HIGH"
check_pattern "create_program_address.*bump" "Client-provided bump" "HIGH"

# Medium severity patterns
echo "=== MEDIUM SEVERITY PATTERNS ===" >> "$OUTPUT_FILE"
check_pattern "todo!\|unimplemented!" "Unimplemented code" "MEDIUM"
check_pattern "println!\|print!" "Debug output in production" "MEDIUM"
check_pattern "clone\(\)" "Potentially expensive clones" "MEDIUM"
check_pattern "Vec::new\(\)" "Unbounded vector allocation" "MEDIUM"

# Solana-specific patterns
echo "=== SOLANA-SPECIFIC PATTERNS ===" >> "$OUTPUT_FILE"
check_pattern "invoke\(" "CPI without validation" "HIGH"
check_pattern "system_instruction::transfer" "Direct SOL transfer" "MEDIUM"
check_pattern "msg!" "Logging (compute cost)" "LOW"
check_pattern "borsh::to_vec" "Serialization overhead" "LOW"

echo ""
echo "Scan complete! Results saved to $OUTPUT_FILE"
echo ""
echo "Summary:"
grep -c "CRITICAL" "$OUTPUT_FILE" | xargs echo "  Critical issues:"
grep -c "HIGH" "$OUTPUT_FILE" | xargs echo "  High severity:"
grep -c "MEDIUM" "$OUTPUT_FILE" | xargs echo "  Medium severity:"
grep -c "LOW" "$OUTPUT_FILE" | xargs echo "  Low severity:"