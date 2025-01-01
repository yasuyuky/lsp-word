# lsp-word

lsp-word provides naive word completion using the Language Server Protocol (LSP).

## Features

- Word completion within the same document

## Setup

### Requirements

- Rust (latest stable version)
- Cargo (Rust's package manager)

### Installation

1. Clone the repository:

    ```sh
    git clone https://github.com/yasuyuky/lsp-word.git
    cd lsp-word
    ```

2. Install the project using Cargo:

    ```sh
    cargo install --path .
    ```

## Usage

### Using with Helix Editor

1. Ensure `lsp-word` is installed and available in your PATH.

2. Add the following configuration to your Helix configuration file (`~/.config/helix/languages.toml`):

    ```toml
    [language-server.word]
    command = "lsp-word"

    [[language]]
    name = "your_language_name"
    language-servers = ["other-server", "word"]
    ```

3. Open a file in Helix Editor that matches the language you configured. The LSP server should start automatically and provide word completion.


## License

This project is licensed under the MIT License. See the `LICENSE` file for details.