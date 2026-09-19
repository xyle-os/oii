" OII syntax. Brackets scope.
if exists('b:current_syntax')
  finish
endif
let b:current_syntax = 'oii'

" comments first so URLs in strings don't break
syntax match oiiLineComment '//.*$'
syntax match oiiDocComment '///.*$'
syntax region oiiBlockComment start='/\*' end='\*/'

" impt goes first
syntax keyword oiiImpt impt

" bools and null
syntax keyword oiiBool true false
syntax keyword oiiNull null

" raw string is verbatim, no escapes inside
syntax region oiiRaw matchgroup=oiiRawDelim start='#"' end='"#'

" cooked string with escapes and {var}
syntax region oiiString matchgroup=oiiStrDelim start='"' end='"' contains=oiiEscape,oiiInterp,oiiBadEscape
syntax match oiiEscape contained '\\\(n\|r\|t\|0\|\\\|\|"\|x[0-9A-Fa-f]\{2}\|u{[0-9A-Fa-f]\+}\)'
syntax match oiiBadEscape contained '\\.'
syntax region oiiInterp matchgroup=oiiBrace start='{' end='}' contained contains=oiiVar
syntax match oiiVar contained '[A-Za-z_][A-Za-z0-9_]*'

" numbers. hex/oct/bin before plain int.
syntax match oiiHex '\<[+-]\?0[xX][0-9A-Fa-f_]\+\>'
syntax match oiiOct '\<[+-]\?0[oO][0-7_]\+\>'
syntax match oiiBin '\<[+-]\?0[bB][01_]\+\>'
syntax match oiiFloat '\<[+-]\?\(\d[\d_]*\.\d[\d_]*\([eE][+-]\?\d[\d_]*\)\?\|\d[\d_]*[eE][+-]\?\d[\d_]*\|\.\d[\d_]*\([eE][+-]\?\d[\d_]*\)\?\|\d[\d_]*\.\)'
syntax match oiiInt '\<[+-]\?\d[\d_]*\>'

" braces do nothing in oii, flag them
syntax match oiiBadBrace '[{}]'

highlight default link oiiLineComment Comment
highlight default link oiiDocComment Comment
highlight default link oiiBlockComment Comment
highlight default link oiiImpt Keyword
highlight default link oiiBool Boolean
highlight default link oiiNull Constant
highlight default link oiiRaw String
highlight default link oiiString String
highlight default link oiiStrDelim String
highlight default link oiiRawDelim String
highlight default link oiiEscape SpecialChar
highlight default link oiiBadEscape Error
highlight default link oiiInterp Identifier
highlight default link oiiVar Identifier
highlight default link oiiHex Number
highlight default link oiiOct Number
highlight default link oiiBin Number
highlight default link oiiFloat Float
highlight default link oiiInt Number
highlight default link oiiBadBrace Error
