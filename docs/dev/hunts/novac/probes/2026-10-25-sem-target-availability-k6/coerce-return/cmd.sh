#!/bin/sh
set -eu
python3 "$(dirname "$0")/probe.py" "${1:-.}"
