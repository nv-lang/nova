#!/bin/sh
# С3 | as | conversions-static-form-stayed
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'This pair **stayed a static form**' spec/conversions.md
grep -nF -- 'receiver form since 2026-09-05' spec/conversions.md
