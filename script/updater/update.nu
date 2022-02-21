#!/usr/local/bin/nu

rustup update
cargo install-update --all

let starshipReal = (readlink ~/.config/starship.toml)
let swissDir = (dirname (dirname $starshipReal))

cd $swissDir

cd submodules/helix
git checkout master
git fetch
git pull
git submodule update --init --recursive
cargo install --path helix-term