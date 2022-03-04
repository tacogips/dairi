## dairi

A neovim plugin that send cmd to background process(e.g. julia repl).

this repo's status is just Poc now.

![preview](https://github.com/tacogips/dairi/blob/main/doc/dairi_preview.gif?raw=true)

## Why not quickrun?

If you run some cmd, say, `using CSV` on julia via quick run, it takes bunch of time to compile it each time. it's such annoying overhead if you like write on neovim and eval it iteration, rather than on julia repl or jupyter notebook.

## Dependencies
- rust (cargo) >= 1.58
- neovim >= 0.5

## Install

with Packer
```
use({ "tacogips/dairi", run = "make install" })
```

## Process management
It's naive and plain and simple.

![process](https://github.com/tacogips/dairi/blob/main/doc/process.jpg?raw=true)
