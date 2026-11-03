#!/bin/bash
# Structural release contracts. Actual built-artifact acceptance runs in release-build.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 -m unittest discover -s .github/tests -p 'test_release_workflow.py'
