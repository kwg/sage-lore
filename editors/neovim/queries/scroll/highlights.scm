; Scroll Assembly tree-sitter highlight queries
; These map tree-sitter node types to Neovim highlight groups.
; Requires a tree-sitter grammar for Scroll Assembly (derived from pest PEG).

; Keywords
["scroll" "type" "require" "provide" "description" "set"] @keyword
["if" "else" "for" "in" "while" "match" "break" "concurrent"] @keyword.control
["continue" "retry" "fallback"] @keyword.exception

; Primitive functions
["invoke" "parallel" "consensus" "run" "elaborate" "distill" "validate" "convert" "aggregate"] @function.builtin

; Primitive namespaces
["platform" "fs" "vcs" "test"] @module

; Types
["str" "int" "float" "bool" "map"] @type.builtin
(type_name) @type
"[]" @type
"?" @type

; Constants
["true" "false"] @boolean
"null" @constant.builtin

; Operators
"->" @operator
"|" @operator
"??" @operator
"++" @operator
"++=" @operator
"=>" @punctuation.special
"&&" @operator
"||" @operator
["==" "!=" ">=" "<=" ">" "<"] @operator
["=" "+=" "-="] @operator

; Strings
(string_lit) @string
(raw_string_lit) @string
(escape_seq) @string.escape
(interpolation) @string.special

; Numbers
(integer_lit) @number
(float_lit) @number.float

; Comments
(COMMENT) @comment

; Identifiers
(identifier) @variable
(ident) @variable.member

; Punctuation
["{" "}"] @punctuation.bracket
["[" "]"] @punctuation.bracket
["(" ")"] @punctuation.bracket
";" @punctuation.delimiter
"," @punctuation.delimiter
":" @punctuation.delimiter
