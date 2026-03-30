" Vim syntax file for Scroll Assembly language (.scroll)
" Language: Scroll Assembly (SAGE Method)

if exists("b:current_syntax")
  finish
endif

" Comments
syn match scrollComment "//.*$" contains=scrollTodo
syn keyword scrollTodo TODO FIXME XXX HACK contained

" Keywords
syn keyword scrollKeyword scroll type require provide description set
syn keyword scrollControl if else for in while match break concurrent
syn keyword scrollErrorHandling continue retry fallback

" Primitive namespaces
syn match scrollNamespace "\<\(platform\|fs\|vcs\|test\)\>\ze\s*\."
" Primitive functions
syn keyword scrollFunction invoke parallel consensus run elaborate distill validate convert aggregate

" Types
syn keyword scrollPrimType str int float bool map
syn match scrollTypeName "\<[A-Z][a-zA-Z0-9_]*\>"
syn match scrollArrayType "\[\]"
syn match scrollNullable "?\ze[^?:]"

" Constants
syn keyword scrollConstant true false null

" Operators
syn match scrollBinding "->"
syn match scrollErrorChain "|\ze[^|]"
syn match scrollNullCoalesce "??"
syn match scrollConcat "++\ze[^+=]"
syn match scrollAppendAssign "++="
syn match scrollMatchArm "=>"
syn match scrollLogical "&&\|||"
syn match scrollComparison "==\|!=\|>=\|<=\|>\|<"

" Numbers
syn match scrollFloat "\<\d\+\.\d\+\>"
syn match scrollInteger "\<\d\+\>"

" Strings with interpolation
syn region scrollString start=/"/ skip=/\\"/ end=/"/ contains=scrollEscape,scrollInterpolation
syn match scrollEscape "\\[\"\\nrt{}0]" contained
syn region scrollInterpolation start=/{/ end=/}/ contained contains=scrollIdentifier,scrollNamespace
syn region scrollRawString start=/`/ end=/`/

" Struct/map fields in literals
syn match scrollFieldName "\<[a-z_][a-zA-Z0-9_]*\>\ze\s*:" contained containedin=scrollStructLit
syn region scrollStructLit start="\<[A-Z][a-zA-Z0-9_]*\>\s*{" end="}" transparent contains=ALL

" Highlighting links
hi def link scrollComment Comment
hi def link scrollTodo Todo
hi def link scrollKeyword Keyword
hi def link scrollControl Conditional
hi def link scrollErrorHandling Exception
hi def link scrollNamespace Type
hi def link scrollFunction Function
hi def link scrollPrimType Type
hi def link scrollTypeName Type
hi def link scrollArrayType Type
hi def link scrollNullable Type
hi def link scrollConstant Constant
hi def link scrollBinding Operator
hi def link scrollErrorChain Operator
hi def link scrollNullCoalesce Operator
hi def link scrollConcat Operator
hi def link scrollAppendAssign Operator
hi def link scrollMatchArm Operator
hi def link scrollLogical Operator
hi def link scrollComparison Operator
hi def link scrollFloat Number
hi def link scrollInteger Number
hi def link scrollString String
hi def link scrollEscape SpecialChar
hi def link scrollInterpolation Special
hi def link scrollRawString String
hi def link scrollFieldName Identifier

let b:current_syntax = "scroll"
