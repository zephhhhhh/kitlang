# Kitlang

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)
[![Rust](https://github.com/zephhhhhh/kitlang/actions/workflows/rust.yml/badge.svg)](https://github.com/zephhhhhh/kitlang/actions/workflows/rust.yml)

A statically-typed, Rust-inspired programming language designed for embedding as a plugin, extension, or modding language.

> **Note**: Kitlang is a hobby project for personal use and demonstrating compiler construction principles. Kitlang is still a work in progress and many features are either not implemented entirely, only partially implemented, or is still in progress of being tested and worked on. Kitlang is _**NOT**_ ready for any serious application in it's current state and using it as such may lead to instability and undefined behaviour.

## Features

-   **Rust-like Syntax**: Familiar syntax for developers with Rust experience.
-   **Static Type System**: Strong type checking with type inference support.
-   **Flexible**: Separation of data and code, does not force object-orientation.
-   **Embeddable**: Provides an easy API for binding functions for interoperability between Kitlang and a host environment.

## Example Kitlang Code

```rust
pub struct Vec2 {
    x: f32,
    y: f32,
}

impl Vec2 {
    pub fn distance_sqr(self, other: Vec2) -> f32 {
        let x_diff = other.x - self.x;
        let y_diff = other.y - self.y;
        (x_diff * x_diff) + (y_diff * y_diff)
    }

    pub fn distance(self, other: Vec2) -> f32 {
        self.distance_sqr(other).sqrt()
    }
}

pub fn main() {
    let coord_1 = Vec2 { x: 5.0, y: 0.0 };
    let coord_2 = Vec2 { x: 10.0, y: 10.0 };

    let distance = coord_1.distance(coord_2);

    println("Distance: " + distance.to_string());
}
```

## Getting Started

### Prerequisites

-   Rust 2024 edition (nightly is currently required for some features)
-   Cargo

### Installation

Add Kitlang to your `Cargo.toml`:

```toml
[dependencies]
kitlang = { git = "https://github.com/zephhhhhh/kitlang" }
```

Or clone the repository:

```bash
git clone https://github.com/zephhhhhh/kitlang.git
cd kitlang
cargo build --release
```

### Basic Usage

```rust
fn main() {
    // TODO.. Add basic embedded use example..
}
```

## Architecture

Kitlang follows a traditional multi-pass compiler architecture:

### 1. **Lexer** (Tokenization)

-   Converts raw source code into a stream of tokens for syntactic analysis.

### 2. **Parser** (Syntactic Analysis)

-   Builds an Abstract Syntax Tree (AST) from tokens. At this stage, identifiers are unresolved strings, representing the structure but not the semantic meaning.

### 3. **HIR Lowering** (High-level Intermediate Representation)

-   Builds a definition registry for name resolution
-   Resolves paths (e.g., `use` statements, `Module::Struct::method`)
-   Constructs scope chains for local variables and function parameters

### 4. **Type Checking**

-   Validates type correctness for all operations
-   Ensures function signatures match call sites
-   Verifies assignment type compatibility
-   Performs type inference for variables declared with no specified type

### 5. **MIR Lowering** (Mid-level Intermediate Representation)

-   Transforms HIR into a simplified representation closer to assembly language, suitable for optimization passes, runtime interpretation and in future, compilation down into a native executable.

### 6. **Interpretation / Code Generation**

-   Currently executes MIR directly via an interpreter. Future plans include lowering to assembly for native execution.

## Project Structure

```
kitlang/
└── src/
    ├── token.rs           # Token definitions
    ├── lexer.rs           # Tokenizer
    ├── ast.rs             # Abstract Syntax Tree definitions
    ├── parser/            # Syntax analysis (Tokens -> AST)
    │
    ├── intermediate/
    │   ├── hir/           # High-level IR
    │   ├── mir/           # Mid-level IR
    │   ├── resolver/      # Name resolution, type resolution, visibility checking, etc.
    │   └── type_check.rs  # Type system checking
    │
    ├── interpreter/       # MIR interpreter
    │
    └── spanned_error.rs   # Error message formatting
```

## Development Status and roadmap

See [here](TODOs.md) for detailed roadmap and priorities.

## Testing

Run the test suite:

```bash
cargo test --tests
```

Run specific test modules:

```bash
cargo test lexer
```

## Documentation

-   [Syntax Reference](Syntax.md) - Language syntax specification
-   [MIR Desugaring](mir_desugar.md) - MIR transformation details

### Generate and view locally

To generate and open the project documentation locally you can run:

```bash
cargo doc --document-private-items --open
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgements

-   Inspired by the syntax of the [Rust programming language](https://www.rust-lang.org/)
-   Rough project and compiler architecture influenced by the [rustc compiler](https://github.com/rust-lang/rust)
