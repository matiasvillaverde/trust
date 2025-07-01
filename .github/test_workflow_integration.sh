#!/bin/bash

# Integration tests for GitHub Actions workflow validation
# Tests the complete workflow structure and dependencies

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

# Function to check if a pattern exists in the workflow
check_workflow_pattern() {
    local pattern="$1"
    local file=".github/workflows/release-on-version-bump.yaml"
    grep -q "$pattern" "$file" && echo "true" || echo "false"
}

# Function to validate YAML syntax
validate_yaml() {
    local file="$1"
    python3 -c "import yaml; yaml.safe_load(open('$file'))" 2>/dev/null && echo "true" || echo "false"
}

echo -e "${YELLOW}Running Workflow Integration Tests${NC}"
echo "======================================="

# Test 1: Workflow file exists
if [ -f ".github/workflows/release-on-version-bump.yaml" ]; then
    run_test "Workflow file exists" "true" "true"
else
    run_test "Workflow file exists" "true" "false"
fi

# Test 2: Valid YAML syntax
result=$(validate_yaml ".github/workflows/release-on-version-bump.yaml")
run_test "Valid YAML syntax" "true" "$result"

# Test 3: Contains safe version parsing (no sed)
result=$(check_workflow_pattern "cut -d'\"' -f2")
run_test "Uses safe version parsing (cut)" "true" "$result"

# Test 4: Does not contain vulnerable sed command
result=$(check_workflow_pattern "sed.*version.*\\\\1")
run_test "No vulnerable sed commands" "false" "$result"

# Test 5: Uses gh CLI for release creation
result=$(check_workflow_pattern "gh release create")
run_test "Uses gh CLI for release creation" "true" "$result"

# Test 6: Uses gh CLI for asset upload
result=$(check_workflow_pattern "gh release upload")
run_test "Uses gh CLI for asset upload" "true" "$result"

# Test 7: Does not use deprecated actions
deprecated_actions=("actions/create-release@v1" "actions/upload-release-asset@v1")
deprecated_found="false"
for action in "${deprecated_actions[@]}"; do
    if grep -q "$action" ".github/workflows/release-on-version-bump.yaml"; then
        deprecated_found="true"
        break
    fi
done
run_test "No deprecated GitHub Actions" "false" "$deprecated_found"

# Test 8: Proper job dependencies
result=$(check_workflow_pattern "needs:.*check-version")
run_test "Proper job dependencies" "true" "$result"

# Test 9: Correct trigger conditions
result=$(check_workflow_pattern "Cargo.toml")
run_test "Triggered on Cargo.toml changes" "true" "$result"

# Test 10: Required permissions
result=$(check_workflow_pattern "contents: write")
run_test "Has required permissions" "true" "$result"

# Test 11: Matrix strategy for cross-compilation
result=$(check_workflow_pattern "matrix:")
run_test "Uses matrix strategy for builds" "true" "$result"

# Test 12: All target platforms included
targets=("x86_64-unknown-linux-gnu" "aarch64-apple-darwin" "x86_64-apple-darwin")
all_targets_found="true"
for target in "${targets[@]}"; do
    if ! grep -q "$target" ".github/workflows/release-on-version-bump.yaml"; then
        all_targets_found="false"
        break
    fi
done
run_test "All target platforms included" "true" "$all_targets_found"

# Test 13: Uses official actions with pinned versions
result=$(check_workflow_pattern "actions/checkout@v4")
run_test "Uses pinned action versions" "true" "$result"

# Test 14: Workflow ends with newline
if [ "$(tail -c 1 .github/workflows/release-on-version-bump.yaml | wc -l)" -eq 1 ]; then
    run_test "File ends with newline" "true" "true"
else
    run_test "File ends with newline" "true" "false"
fi

# Test 15: Environment variables properly set
result=$(check_workflow_pattern "GITHUB_TOKEN.*secrets.GITHUB_TOKEN")
run_test "GitHub token properly configured" "true" "$result"

echo ""
echo "======================================="
echo -e "${YELLOW}Integration Test Summary${NC}"
echo "Tests run: $TESTS_RUN"
echo -e "Tests passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Tests failed: ${RED}$((TESTS_RUN - TESTS_PASSED))${NC}"

if [ $TESTS_PASSED -eq $TESTS_RUN ]; then
    echo -e "${GREEN}All integration tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some integration tests failed!${NC}"
    exit 1
fi