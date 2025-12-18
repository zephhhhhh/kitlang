# To-do list
## Parser
- Improve type parsing.
- Implement array expressions/types.
- Implement tuple expressions/types.

## HIR
### Eventually:
- Implement enums and match statements.

# Timeline
## High priority
- Add better API for actually loading and running kitlang code files and source code.
- Add documentation for the `Visitor` traits.
- Create tests for MIR lowering.

## Medium priority
- Revisit renaming especially for parser.
- Add proper logging for things that are not diagnostic but for compiler debugging.

## Low Priority
- Update syntax docs.
- Add proper configuration (This will be especially important to add when working on the `Use` statement)
- Update README with embedded use example.