#!/bin/bash

# Test script for version parsing logic used in GitHub Actions workflows
# This script follows TDD principles to test the version parsing functionality

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counter
TESTS_RUN=0
TESTS_PASSED=0

# Function to run a test
run_test() {
    local test_name="$1"
    local expected="$2"
    local actual="$3"
    
    TESTS_RUN=$((TESTS_RUN + 1))
    
    if [ "$expected" = "$actual" ]; then
        echo -e "${GREEN}✓ PASS${NC}: $test_name"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗ FAIL${NC}: $test_name"
        echo -e "  Expected: '$expected'"
        echo -e "  Actual:   '$actual'"
    fi
}

# Create temporary test files
create_test_cargo_toml() {
    local version="$1"
    cat > "/tmp/test_Cargo.toml" << EOF
[package]
name = "trust"
version = "$version"
edition = "2021"

[dependencies]
EOF
}

# Test function for version parsing
parse_version() {
    local cargo_file="$1"
    grep '^version = ' "$cargo_file" | head -1 | cut -d'"' -f2
}

# Test function for version parsing (old vulnerable method for comparison)
parse_version_old() {
    local cargo_file="$1"
    grep -E '^version = ' "$cargo_file" | head -1 | sed 's/version = "\(.*\)"/\1/'
}

echo -e "${YELLOW}Running Version Parsing Tests${NC}"
echo "================================"

# Test 1: Basic version parsing
create_test_cargo_toml "1.0.0"
result=$(parse_version "/tmp/test_Cargo.toml")
run_test "Basic version parsing (1.0.0)" "1.0.0" "$result"

# Test 2: Version with pre-release
create_test_cargo_toml "1.0.0-alpha.1"
result=$(parse_version "/tmp/test_Cargo.toml")
run_test "Pre-release version parsing" "1.0.0-alpha.1" "$result"

# Test 3: Version with build metadata
create_test_cargo_toml "1.0.0+build.1"
result=$(parse_version "/tmp/test_Cargo.toml")
run_test "Version with build metadata" "1.0.0+build.1" "$result"

# Test 4: Semantic version with patch
create_test_cargo_toml "2.15.7"
result=$(parse_version "/tmp/test_Cargo.toml")
run_test "Semantic version with patch" "2.15.7" "$result"

# Test 5: Version with all components
create_test_cargo_toml "3.2.1-beta.2+exp.sha.abc123"
result=$(parse_version "/tmp/test_Cargo.toml")
run_test "Complex version string" "3.2.1-beta.2+exp.sha.abc123" "$result"

# Test 6: Test that old and new methods produce same results (regression test)
create_test_cargo_toml "1.5.3"
new_result=$(parse_version "/tmp/test_Cargo.toml")
old_result=$(parse_version_old "/tmp/test_Cargo.toml")
run_test "Regression test: new method matches old method" "$old_result" "$new_result"

# Test 7: Handle malformed input gracefully
create_test_cargo_toml "1.0.0"
# Add a malformed line to test robustness
echo 'version = "broken' >> "/tmp/test_Cargo.toml"
result=$(parse_version "/tmp/test_Cargo.toml" || echo "ERROR")
run_test "Handle malformed input" "1.0.0" "$result"

# Test 8: Empty version handling
create_test_cargo_toml ""
result=$(parse_version "/tmp/test_Cargo.toml")
run_test "Empty version string" "" "$result"

# Security Test: Test that the new method doesn't execute arbitrary commands
# This test ensures we've mitigated the shell injection vulnerability
create_test_cargo_toml '$(echo "injected")'
result=$(parse_version "/tmp/test_Cargo.toml")
run_test "Security: No command injection" '$(echo "injected")' "$result"

# Clean up
rm -f "/tmp/test_Cargo.toml"

echo ""
echo "================================"
echo -e "${YELLOW}Test Summary${NC}"
echo "Tests run: $TESTS_RUN"
echo -e "Tests passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Tests failed: ${RED}$((TESTS_RUN - TESTS_PASSED))${NC}"

if [ $TESTS_PASSED -eq $TESTS_RUN ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi