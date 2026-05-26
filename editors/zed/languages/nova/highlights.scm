; Nova tree-sitter highlights
; Capture names follow the tree-sitter standard set.

; ─── Keywords ────────────────────────────────────────────────────────────────

; Note: "break" and "continue" are named nodes, not anonymous tokens.
; All other keywords below are anonymous tokens accessible in queries.
[
  "module" "import" "use" "export" "external"
  "fn" "type" "effect" "alias"
  "let" "const" "mut" "consume" "readonly"
  "if" "else" "match" "for" "while" "loop" "in"
  "return" "test"
  "with" "throw"
  "spawn" "supervised" "parallel" "detach" "blocking"
  "protocol" "forbid" "realtime"
  "defer" "errdefer" "select" "lemma"
] @keyword

; Named-node keywords (defined as standalone rules)
(break_statement) @keyword
(continue_statement) @keyword

; Keyword operators (word-form)
["as" "is" "not" "or" "and"] @keyword.operator

; Boolean literals
(bool_literal) @boolean

; ─── Comments ────────────────────────────────────────────────────────────────

(doc_comment) @comment.documentation
(line_comment) @comment
(block_comment) @comment

; ─── Types ───────────────────────────────────────────────────────────────────

; Any type_name node (used in named_type, sum_variant, record_literal, method receiver)
(type_name) @type

; Type declaration name
(type_declaration name: (type_name) @type.definition)

; Generic type parameters
(generic_param name: (identifier) @type.parameter)

; Effect names in function signatures
(effect_name) @type

; ─── Functions ───────────────────────────────────────────────────────────────

; Free function definition
(fn_declaration
  name: (identifier) @function)

; Instance method definition: fn Type @method(...)
(fn_declaration
  receiver_type: (type_name) @type
  name: (identifier) @function.method)

; Closure / anonymous function keyword
(closure_expr "fn" @keyword)

; Function call (bare name)
(call_expr func: (identifier) @function.call)

; Method call via field access: obj.method(...)
(call_expr
  func: (field_expr
    field: (identifier) @function.method.call))

; ─── Variables ───────────────────────────────────────────────────────────────

(let_declaration name: (identifier) @variable)
(let_statement name: (identifier) @variable)
(const_declaration name: (identifier) @constant)

; Function parameters
(param name: (identifier) @variable.parameter)

; Test name (string)
(test_declaration name: (string_literal) @string.special)

; ─── Self ─────────────────────────────────────────────────────────────────────

(self_expr) @variable.builtin
(self_field_expr) @variable.member
(self_field_expr field: (identifier) @variable.member)

; ─── Fields and properties ───────────────────────────────────────────────────

; Field access: obj.field
(field_expr field: (identifier) @property)

; Struct field definition
(record_field name: (identifier) @property)

; Record literal field initializer
(field_init name: (identifier) @property)

; ─── Literals ────────────────────────────────────────────────────────────────

(number_literal) @number
(char_literal) @character

(string_literal) @string
(string_interpolation
  "${" @punctuation.special
  "}" @punctuation.special)

; ─── Operators ───────────────────────────────────────────────────────────────

[
  "+" "-" "*" "/" "%"
  "&" "|" "^"
  "<<" ">>"
  "&&" "||" "??" "!"
  "==" "!=" "<" "<=" ">" ">="
  "=" "+=" "-=" "*=" "/="
  ".." "..="
  "==>" "<==>"
] @operator

; Try and unwrap operators
["?" "!!"] @operator

; ─── Attributes ──────────────────────────────────────────────────────────────

(item_attribute "#" @punctuation.special name: (identifier) @attribute)

; ─── Punctuation ─────────────────────────────────────────────────────────────

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," "." ":"] @punctuation.delimiter
["=>" "->"] @punctuation.special
["@"] @punctuation.special
