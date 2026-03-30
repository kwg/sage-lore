# Editor Support for Scroll Assembly

Syntax highlighting and language support for `.scroll` files (Scroll Assembly language).

## VS Code

```bash
# Symlink the extension into your VS Code extensions directory
ln -s $(pwd)/vscode/sage-scroll ~/.vscode/extensions/sage-scroll

# Or copy it
cp -r vscode/sage-scroll ~/.vscode/extensions/
```

Provides: syntax highlighting, bracket matching, code folding, snippets.

## Vim / Neovim (traditional)

```bash
# Add to your vim config
mkdir -p ~/.vim/syntax ~/.vim/ftdetect
cp vim/syntax/scroll.vim ~/.vim/syntax/
cp vim/ftdetect/scroll.vim ~/.vim/ftdetect/

# For Neovim
mkdir -p ~/.config/nvim/syntax ~/.config/nvim/ftdetect
cp vim/syntax/scroll.vim ~/.config/nvim/syntax/
cp vim/ftdetect/scroll.vim ~/.config/nvim/ftdetect/
```

Provides: syntax highlighting, filetype detection.

## Neovim (tree-sitter)

The `neovim/queries/scroll/highlights.scm` file provides highlight queries for
tree-sitter. Requires a tree-sitter grammar for Scroll Assembly (to be derived
from the pest PEG grammar at `src/scroll/assembly/scroll_assembly.pest`).

For now, use the traditional vim syntax file above.

## Kate (KDE)

```bash
# Install the syntax definition
mkdir -p ~/.local/share/org.kde.syntax-highlighting/syntax/
cp kate/scroll.xml ~/.local/share/org.kde.syntax-highlighting/syntax/
```

Provides: syntax highlighting, comment toggling, code folding.

## Language Features

All editors support highlighting for:

- **Keywords**: `scroll`, `type`, `require`, `provide`, `set`, `description`
- **Control flow**: `if`, `else`, `for`, `in`, `while`, `match`, `break`, `concurrent`
- **Error handling**: `continue`, `retry`, `fallback`
- **Primitives**: `invoke`, `parallel`, `consensus`, `run`, `elaborate`, `distill`, `validate`, `convert`, `aggregate`
- **Namespaces**: `platform.`, `fs.`, `vcs.`, `test.`
- **Types**: `str`, `int`, `float`, `bool`, `map`, `TypeName`, `T[]`, `T?`
- **Operators**: `->`, `|`, `??`, `++`, `++=`, `=>`, `&&`, `||`, comparisons
- **Strings**: `"interpolated {var}"`, `` `raw strings` ``, escape sequences
- **Comments**: `// line comments`

## LSP Server

An LSP server for `.scroll` files is planned (#138). It will provide:
- Diagnostics (type errors, undefined variables, non-exhaustive match)
- Autocomplete for primitives, types, and variables
- Hover documentation
- Go-to-definition for variables and type references

The LSP will reuse the parser and type checker from `src/scroll/assembly/`.
