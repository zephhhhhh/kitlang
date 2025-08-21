Type = `<Identifier>`

IdentifierPattern -> `mut? <Identifier>`



ReturnExpression -> `return <Expression>`
ArrayExpression -> `[<Expression> (, <Expression>)*]`

ExpressionWithoutBlock -> `<Literal> | <ReturnExpression> | <ArrayExpression>`

Statements -> `<Statement>+ | <Statement>+ <ExpressionWithoutBlock> | <ExpressionWithoutBlock>`
BlockExpression -> `{ <Statements> }`

ExpressionWithBlock -> `<BlockExpression>`

Expression -> `<ExpressionWithoutBlock> | <ExpressionWithBlock>`


SelfParam -> `&? mut? self`
FunctionParam -> `mut? <Identifier>: &? mut? <Type>`
FunctionParameters -> `<SelfParam> | (<SelfParam>,)? <FunctionParam> (, <FunctionParam>)*`
FunctionReturnType -> `-> <Type>`
Function -> `pub? fn <Identifier>(<FunctionParameters>?) <FunctionReturnType>? <BlockExpression>`

Item -> `<Function> | <Struct> | <Implementation>`
LetStatement -> `let <IdentifierPattern> : <Type> = <Expression>;`
Statement -> `; | <Item> | <LetStatement> | <ExpressionStatement>`
