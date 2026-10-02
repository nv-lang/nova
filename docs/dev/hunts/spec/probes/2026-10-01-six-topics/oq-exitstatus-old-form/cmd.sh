#!/bin/sh
# С3 | variants | oq-exitstatus-old-form
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'type ExitStatus | Ok | Failure | Critical' spec/open-questions.ru.md
grep -nF -- 'type ExitStatus enum Ok | Failure | Critical' spec/syntax.ru.md
