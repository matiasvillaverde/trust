#!/bin/bash
set -euo pipefail
python3 -m unittest discover -s .github/tests -p 'test_*.py'
