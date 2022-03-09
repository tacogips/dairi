## dairi

A neovim plugin that send input to stdio of background process(e.g. julia repl).

It's state of PoC now . Operation confirmed on only linux.


![preview](https://github.com/tacogips/dairi/blob/main/doc/dairi_preview.gif?raw=true)

## Why not quickrun?

If you run some cmd, say, `using CSV` on julia via quick run, it takes bunch of time to compile it each time. there are such annoying overheads if you like write on neovim and eval it iteration, rather than on julia repl or jupyter notebook.

## Dependencies
- rust (cargo) >= 1.58
- neovim >= 0.5

## Install

with Packer
```
use({ "tacogips/dairi", run = "make install" })
```

## Usage

at first invoke dairi-server
```
dairi-server
```
the the default config file will be created at `$HOME/.config/dairi/config.toml` with contents below

```toml
[[cmds]]
name = "julia"
cmd = "julia"
output_size = 4096
join_input_newline_with = ";"
auto_trailing_newline = true
truncate_line_regex = "#.*"
remove_empty_line = true
no_empty_input = true
timeout_sec = 120
wait_output_timeout_milli_sec = 500
```

### setup on neovim(lua)

```lua
-- cmd table reposerents { file_type = cmd_name}
require("dairi_run").setup({
  cmds = {
     julia = "julia",
  },
})
```

then open julia file and send the buffer to julia repl process which invoked behind dairi-server

```
:lua require('dairi_run').run()
```


## (Supplement) Process management
It's naive, plain and simple.

![process](https://github.com/tacogips/dairi/blob/main/doc/process.jpg?raw=true)
