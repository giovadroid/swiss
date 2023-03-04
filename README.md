# SWISS

The best cross-platform shell and tools for daily usage. All built it in rust.

Swiss aims to simplify operating system switching for console users. It uses nushell as the default terminal available
on linux, windows and mac.

It aims to offer a series of commands to facilitate the maintenance, or boostraping, of the computers we manage.

# Install

To install just run in your terminal:

```
git clone https://gitlab.com/jaesbit/swiss.git
cd swiss
cargo install --path .
swiss setup
```

Then you need to wait until swiss prepare your system to work with the swiss default tools

# What comes by default

The commands are defined with the following format

* [$name ($command1, $command2, ...)]($url_to_documentation) $description
* [$command]($url_to_documentation) $description
* [$command ($alias1, $alias2, ...)]($url_to_documentation) $description

## Core tools

* [Nushell (nu)](https://www.nushell.sh/)
* [helix (hx)](https://github.com/helix-editor/helix) shell editor like `vim` but more user friendly
* [zoxide (cd, cdi, z, zi)](https://github.com/ajeetdsouza/zoxide) faster and intuitive `cd`
* [find-files (ff)](https://crates.io/crates/find-files) faster alternative to `find`
* [ripgrep (rg)](https://github.com/BurntSushi/ripgrep) `grep` alternative
* [starship](https://github.com/starship/starship) cross shell prompt

## Dev shell tools

* [watchexec (wexec)](https://github.com/watchexec/watchexec) files watcher util when develop and perform any shell
  command on changes
* [hexyl (hx)](https://github.com/sharkdp/hexyl) command line hex viewer
  * **TODO Installation is not implemented yet please doit manually**
* [tokei](https://github.com/XAMPPRocky/tokei) Tokei is a program that displays statistics about your code.
* [rust-analyzer](https://github.com/rust-analyzer/rust-analyzer)
  * **TODO Installation is not implemented yet please doit manually**
  * `rustup components add rust-analyzer`

## More cross platform tools

* [tealdeer (tldr)](https://github.com/dbrgn/tealdeer) fast implementation of tldr
* [dua](https://github.com/Byron/dua-cli) Disk usage analyzer shell or interactive with `i` arg, you can also delete files and folders from interactive mode
* [czkawka_cli](https://github.com/qarmin/czkawka) file and folder comparer and more
* [grex](https://github.com/pemistahl/grex) command-line tool and library for generating regular expressions
* [websocat](https://github.com/vi/websocat) Netcat, curl and socat for WebSockets.
* [bat](https://github.com/sharkdp/bat) `cat` alternative with git integration
* [procs](https://github.com/dalance/procs) `ps` altertanive cross platform
* [sd](https://crates.io/crates/sd) is an intuitive find & replace CLI. alternative to `awk` and `sed`
* [hyperfine](https://github.com/sharkdp/hyperfine) A command-line benchmarking tool.
* [bottom (btm)](https://crates.io/crates/bottom) A customizable cross-platform graphical process/system monitor for the
  terminal
* [bandwich](https://github.com/imsnif/bandwhich) CLI utility for displaying current network utilization by process
* [rmesg](https://github.com/polyverse/rmesg) A `dmesg` implementation in Rust
* [git-delta](https://github.com/dandavison/delta) nice diff viewer integrated in git
  * ** TODO: Pending to complete integration with git requires to do it manually**
* [fnm](https://github.com/Schniz/fnm) Fast and simple Node.js version manager, built in RustFast node manager
* [gitui](https://github.com/extrawurst/gitui) Blazing fast terminal-ui for git written in rust

## Cargo extra tools

All this commands are for cargo, you can use them with `cargo $command`. The `$command` is between parenthesis.

* [watch (watch)](https://github.com/watchexec/cargo-watch)
* [update (install-update)](https://github.com/nabijaczleweli/cargo-update)
* [edit (add)](https://github.com/killercup/cargo--edit)
* [cache (cache)](https://crates.io/crates/cargo-cache)
* [deps (deps)](https://crates.io/crates/cargo-deps)

## Linux or Mac Os only tools

* [zellij (zj, zellij)](https://github.com/zellij-org/zellij) terminal multiplexer like `tmux` but more user friendly
* [exa (exa, els)](https://github.com/ogham/exa) like `ls`

# Customization

when setup is completed you will have a `~/.swiss` folder with the following structure

* `~/.swiss` folder where you can add custom yaml files to be loaded on startup on `swiss init` call just
  before `~/.swiss/env/*.nu`
  * `env` folder where you can add your custom nu scripts that will be loaded on startup on `env` call
  * `conf` folder where you can add your custom starship config that will be loaded on startup on `conf` call

# Known issues

* Windows `startship timing` and also new line loads will be increased in rust with high ms usage when are in rust
  folder

# TODO Features

- [x] detect os and perform boostraping
- [ ] Add custom yaml implementation
- [ ] Document yaml format
- [ ] add upgrade support
- [ ] reduce installation by cache install compilations
- [ ] add nerd fonts fira and hack