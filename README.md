# SWISS

Cross platform shell and tools for daily usage. All build it in rust
`swiss-help` command will show this `README.md` content

## How it works

You can define nushell as default shell in linux or mac by using `chsh`, but is not recommended to open issues.

On windows we recomend to use with `Windows Terminal` from `Microsoft Store` and configure with PowerShell configuration. Also all tools are installed in your PATH

You can launch `zj` and start working with `nu` shell with our set of scripts availables on `ls ~/.swiss/commands/` also all are on `help` command from `nu`.

You can learn about each tool installed by following some of the next links.

## Core tools

* [Nushell (nu)](https://www.nushell.sh/)
* [starship](https://github.com/starship/starship) cross shell prompt
* [helix (hx)](https://github.com/helix-editor/helix) shell editor like `vim` but more user friendly
* [zoxide (cd, cdi, z, zi)](https://github.com/ajeetdsouza/zoxide) faster and intuitive `cd`
* [find-files (ff)](https://crates.io/crates/find-files) faster alternative to `find`
* [ripgrep (rg)](https://github.com/BurntSushi/ripgrep) `grep` alternative

## Dev shell tools

* [watchexec](https://github.com/watchexec/watchexec) files watcher util when develop and perform any shell command on changes
* [hexyl](https://github.com/sharkdp/hexyl) command line hex viewer
 * TODO Installation is disabled
* [tokei](https://github.com/XAMPPRocky/tokei) Tokei is a program that displays statistics about your code.
* [rust-analyzer](https://github.com/rust-analyzer/rust-analyzer)
 * TODO Installation is disabled

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
* [bottom (btm)](https://crates.io/crates/bottom) A customizable cross-platform graphical process/system monitor for the terminal
* [bandwich](https://github.com/imsnif/bandwhich) CLI utility for displaying current network utilization by process
* [rmesg](https://github.com/polyverse/rmesg) A `dmesg` implementation in Rust 
* [git-delta](https://github.com/dandavison/delta) nice diff viewer integrated in git
* [fnm](https://github.com/Schniz/fnm) Fast and simple Node.js version manager, built in RustFast node manager 
 * TODO: Pending to complete integration in windows
 * TODO: nu startup settings currently is disabled

## Cargo extra tools

* [watch (cargo watch)](https://github.com/watchexec/cargo-watch)
* [update (cargo install-update)](https://github.com/nabijaczleweli/cargo-update)
* [edit (cargo add...)](https://github.com/killercup/cargo-edit)

## Linux or Mac Os tools

* [zellij (zj, zellij)](https://github.com/zellij-org/zellij) terminal multiplexer like `tmux` but more user friendly
* [exa (exa)](https://github.com/ogham/exa) like `ls`

## Our Custom nushell scripts

All our scripts are on help menu

* swiss-*:
 * `watch` util to develop new commands or edit some scripts
 * `update` TODO: working on it, but will try to update all tools
* git tools
 * `gpush`
 * `gpull`
* `extract` will select unpacker based on extension (not check if required tool are installed)

# Known issues

* `exa` as `ls` will throw error on some C/C++ compilations because date are not like `ls`
* Windows `startship timing` and also new line loads will be increased in rust with high ms usage when are in rust folder

# TODO Features

- [x] detect os and perform boostraping
- [ ] add upgrade support
- [ ] reduce installation by cache install compilations
- [ ] add nerd fonts fira and hack