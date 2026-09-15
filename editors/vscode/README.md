# GHL Language Support for VS Code & Positron

Official extension for **GHL** (*Generalized Hypothesis Language* / *Gojo & Haru Language*).

## Features

- **Syntax Highlighting**: Full TextMate grammar for `.gh` and `.ghl` files, covering Kleene NA literals (`NA`, `NA:Reason`), pipe operators (`|>`), statistical formulas (`~`), matrix operations (`\`, `.*`, etc.), canonical verbs (`filter`, `select`, `mutate`, `ols`, `summary`), and types.
- **Language Configuration**: Auto-closing pairs (`()`, `[]`, `{}`, `#[ ... ]`, `""`), bracket matching, indentation rules for dataframes and matrices, and code folding.
- **Ergonomic Snippets**: Instant templates for functions, OLS statistical models, data pipelines, Grammar of Graphics plots, dataframes, matrices, and assertions (`pounce!`).
- **Language Server Protocol (LSP)**: Automatic connection to `ghl lsp` for real-time diagnostics, hover documentation with LaTeX formulas, autocompletion, and definitions.
- **Interactive REPL (`Ctrl + Enter`)**: Send the current line or selected code directly to an interactive `ghl repl` terminal.

## Requirements

The `ghl` CLI executable should be available in your system `PATH` (or configured via `ghl.server.path`).

## Extension Settings

- `ghl.server.path`: Path to the `ghl` binary (default: `"ghl"`).
- `ghl.server.enabled`: Enable or disable the Language Server (default: `true`).
- `ghl.trace.server`: Tracing level for LSP communication (`"off"`, `"messages"`, `"verbose"`).
