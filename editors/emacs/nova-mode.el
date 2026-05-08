;;; nova-mode.el --- Major mode for the Nova programming language -*- lexical-binding: t; -*-

;; Author: Nova Language project
;; Version: 0.1.0
;; Keywords: languages
;; URL: https://github.com/nova-lang/nova-lang (TODO)
;; License: MIT OR Apache-2.0
;; Package-Requires: ((emacs "26.1"))

;;; Commentary:

;; Syntax highlighting for Nova language `.nv` files.
;;
;; Synchronized with editors/vscode/syntaxes/nova.tmLanguage.json
;; Source-of-truth for keywords: compiler-codegen/src/lexer/mod.rs

;;; Code:

(defvar nova-mode-syntax-table
  (let ((table (make-syntax-table)))
    ;; Comments — // line, /* */ block
    (modify-syntax-entry ?/ ". 124b" table)
    (modify-syntax-entry ?* ". 23" table)
    (modify-syntax-entry ?\n "> b" table)
    ;; Strings
    (modify-syntax-entry ?\" "\"" table)
    (modify-syntax-entry ?\\ "\\" table)
    ;; Backtick для tagged templates
    (modify-syntax-entry ?` "\"" table)
    ;; @ — часть идентификатора (для @field)
    (modify-syntax-entry ?@ "_" table)
    ;; _ — обычный символ слова
    (modify-syntax-entry ?_ "w" table)
    table)
  "Syntax table for `nova-mode'.")

;; ============================================================================
;; Keyword groups
;; ============================================================================

(defconst nova-keywords-declaration
  '("fn" "type" "alias" "effect" "handler" "protocol"
    "let" "const" "module" "import" "export" "as" "use" "test"
    "external")
  "Declaration keywords.")

(defconst nova-keywords-modifier
  '("mut" "readonly")
  "Storage modifier keywords.")

(defconst nova-keywords-control
  '("if" "else" "match" "for" "while" "loop"
    "break" "continue" "return" "throw"
    "with" "interrupt" "defer" "in")
  "Control flow keywords.")

(defconst nova-keywords-concurrency
  '("spawn" "detach" "parallel" "supervised" "cancel_scope"
    "race" "select" "with_timeout")
  "Concurrency primitive keywords.")

(defconst nova-keywords-memory
  '("region" "forbid" "realtime")
  "Memory / safety block keywords.")

(defconst nova-keywords-operator
  '("is")
  "Operator-like keywords.")

(defconst nova-keywords-contract
  '("requires" "ensures" "invariant" "old")
  "Contract keywords.")

(defconst nova-keywords-boolean
  '("true" "false")
  "Boolean literals.")

;; ============================================================================
;; Type groups
;; ============================================================================

(defconst nova-prelude-types
  '("Option" "Some" "None" "Result" "Ok" "Err"
    "Ordering" "Less" "Equal" "Greater"
    "Error" "RuntimeError" "Never" "Self"
    "Iter" "Range" "RangeIter"
    "Channel" "CancelToken" "Handler"
    "From" "Into" "TryFrom" "TryInto"
    "Hashable" "Eq" "Ord"
    "StringBuilder" "WriteBuffer" "ReadBuffer" "ReadBufferError")
  "Prelude type names (D26).")

(defconst nova-effects
  '("Fail" "Io" "Net" "Db" "Fs" "Time" "Random"
    "Log" "Trace" "Ask" "Alloc" "Detach" "Blocking" "Mem")
  "Standard effect names (D2, D62).")

(defconst nova-primitive-types
  '("int" "i8" "i16" "i32" "i64"
    "u8" "u16" "u32" "u64"
    "f32" "f64"
    "str" "bool" "char" "byte" "any")
  "Primitive type names.")

;; ============================================================================
;; Font-lock keywords
;; ============================================================================

(defun nova--keyword-regexp (words)
  "Build a word-bounded regexp matching any of WORDS."
  (concat "\\<" (regexp-opt words t) "\\>"))

(defvar nova-font-lock-keywords
  `(
    ;; Strings — handled by syntax table, but interpolation ${...}
    ("\\${[^}]*}" . font-lock-variable-name-face)

    ;; Numeric literals
    ("\\<0x[0-9a-fA-F]\\(_?[0-9a-fA-F]\\)*\\>" . font-lock-constant-face)
    ("\\<0b[01]\\(_?[01]\\)*\\>"               . font-lock-constant-face)
    ("\\<0o[0-7]\\(_?[0-7]\\)*\\>"             . font-lock-constant-face)
    ("\\<[0-9]\\(_?[0-9]\\)*\\(\\.[0-9]\\(_?[0-9]\\)*\\)?\\([eE][+-]?[0-9]\\(_?[0-9]\\)*\\)?\\>"
     . font-lock-constant-face)

    ;; @field / @ — self access
    ("@[a-z_][a-zA-Z0-9_]*"  . font-lock-variable-name-face)
    ("@\\(?:[^a-zA-Z_]\\|$\\)" . font-lock-variable-name-face)

    ;; Char literal: 'a' / '\n' / '\u{1F600}'
    ("'\\(\\\\.\\|[^']\\)'" . font-lock-string-face)

    ;; Keywords
    (,(nova--keyword-regexp nova-keywords-declaration) . font-lock-keyword-face)
    (,(nova--keyword-regexp nova-keywords-modifier)    . font-lock-keyword-face)
    (,(nova--keyword-regexp nova-keywords-control)     . font-lock-keyword-face)
    (,(nova--keyword-regexp nova-keywords-concurrency) . font-lock-keyword-face)
    (,(nova--keyword-regexp nova-keywords-memory)      . font-lock-keyword-face)
    (,(nova--keyword-regexp nova-keywords-operator)    . font-lock-keyword-face)
    (,(nova--keyword-regexp nova-keywords-contract)    . font-lock-preprocessor-face)
    (,(nova--keyword-regexp nova-keywords-boolean)     . font-lock-constant-face)

    ;; Types
    (,(nova--keyword-regexp nova-prelude-types)    . font-lock-type-face)
    (,(nova--keyword-regexp nova-effects)          . font-lock-type-face)
    (,(nova--keyword-regexp nova-primitive-types)  . font-lock-type-face)

    ;; PascalCase identifiers — also types
    ("\\<[A-Z][a-zA-Z0-9_]*\\>" . font-lock-type-face)

    ;; SCREAMING_SNAKE_CASE — constants
    ("\\<[A-Z][A-Z0-9_]+\\>" . font-lock-constant-face)

    ;; Function declaration: fn name / fn Type.method
    ("\\<fn\\>[ \t]+\\([a-z_][a-zA-Z0-9_]*\\)" 1 font-lock-function-name-face)
    ("\\<fn\\>[ \t]+\\([A-Z][a-zA-Z0-9_]*\\)\\.\\([a-z_][a-zA-Z0-9_]*\\)"
     (1 font-lock-type-face) (2 font-lock-function-name-face))
    )
  "Font-lock keywords for Nova mode.")

;; ============================================================================
;; Comments — handled by syntax-table; doc-comments via prefix /// are detected
;; ============================================================================

;; ============================================================================
;; Major mode
;; ============================================================================

;;;###autoload
(define-derived-mode nova-mode prog-mode "Nova"
  "Major mode for editing Nova source code."
  :syntax-table nova-mode-syntax-table

  ;; Comment syntax
  (setq-local comment-start "// ")
  (setq-local comment-end "")
  (setq-local comment-start-skip "//+\\s-*")
  (setq-local comment-use-syntax t)

  ;; Indentation
  (setq-local tab-width 4)
  (setq-local indent-tabs-mode nil)

  ;; Font-lock
  (setq-local font-lock-defaults '(nova-font-lock-keywords nil nil nil nil)))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.nv\\'" . nova-mode))

;; ============================================================================
;; Optional: rainbow-delimiters integration
;; If user has rainbow-delimiters package installed, enable it for nova-mode.
;; ============================================================================

(when (featurep 'rainbow-delimiters)
  (add-hook 'nova-mode-hook #'rainbow-delimiters-mode))

(provide 'nova-mode)
;;; nova-mode.el ends here
