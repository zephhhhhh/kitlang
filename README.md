# Kitlang

Kitlang is a hobby language project inspired by rust, designed to be embedded in programs 
as a plugin/extension/modding language.

TODO...

# Kitlang stages
## Tokeniser
- Tokeniser takes the raw input source code text and transforms it into individual tokens for easier processing.
## Parser
- Parser takes in the output of the tokeniser as input, and performs "syntactic analysis" on the tokens, building out the 
meaning for the sequence of tokens, but at this stage everything is still referenced using strings. Another way of putting this
would be that none of the references to other functions or variables are resolved at this stage, but just that the overall structure
and meaning of the code is laid out in memory. The output of this stage is called the AST (Abstract syntax tree).
## Lowering to HIR (High-level intermediate representation)
- This stage will first build out all the definitions in each file into a data structure for searching and resolving.
- After this we walk the AST and attempt to resolve first the relative paths if used, I.e. `use` statements or `Project::Struct::Function`
references, while we do this we build out a scope structure to keep track of local declarations, function parameters to see if referenced
values are valid. For example that they are referenced after declared and not before etc.
## Type checking
- After everything is resolved, we then do type checking. This ensures all operations on types are valid, function arguments are 
supplied the expected type, the return type matches the declared type, assignments have the same type on both sides, etc.
- This is also where type propagation is performed for "Infer" types.
## Lowering to MIR (Middle-level Intermediate Respresentation) / Optimisation
- This stage would involve further lowering the representation to something closer to what a compiler would ultimately output in assembly,
this form would be easier to optimise, but is optional.
## Code generation / Execution
- Either the type-checking stage or the MIR output is a valid representation to start either generating assembly code output,
or feeding into an interpreter for execution.

# Interpreter
## Notes
The current interpreter implementation is almost entirely subject to change, as it stands the interpreter will interpret the MIR directly. Eventually this MIR will be lowered further into actually assembly instructions before being assembled into a final executable.

# To-do list
## Parser
- Improve type parsing.
- Implement array expressions/types.
- Implement tuple expressions/types.
- Implement type casting expressions.
- Implement assign binary operations.

## HIR
### Resolver
- Factor in "use" statements when resolving.
- Correctly resolve local only references.

### Eventually:
- Implement enums and match statements.

# Timeline
## High priority
- Add proper error handling like in the parser for other stages of the compiler.
- Add documentation for HIR and MIR and interpreter.
- Add documentation for the [`Visitor`] traits.
- Create tests for HIR and MIR lowering.

## Medium priority
- Implement `Use` statement.
- Revisit renaming especially for parser.
- Add proper logging for things that are not diagnostic but for compiler debugging.

## Low Priority
- Update syntax docs.
- Add proper configuration (This will be especially important to add when working on the `Use` statement)