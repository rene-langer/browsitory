### Syntax highlighting

- Changed: syntax highlighting uses a WebAssembly build of the Oniguruma regular expression
  library, about 4 times faster on TypeScript in the desktop app on Linux. The app's content
  security policy now allows compiling WebAssembly (`'wasm-unsafe-eval'`), not `eval` (#843).
