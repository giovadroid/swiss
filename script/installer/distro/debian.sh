#!/bin/bash

sudo apt update

sudo apt install -y \
    build-essential libssl-dev pkg-config docker.io net-tools libglibd-2.0-dev fzf libx11-dev
sudo apt autoremove -y
sudo apt autoclean -y

curl https://sh.rustup.rs -sSf | sh -s -- -y

source $HOME/.cargo/env

 
curl -fsSL https://starship.rs/install.sh | sh -s --  -y
# sh -c "$(curl -fsSL https://starship.rs/install.sh)"
# cargo install starship
cargo install nu --features=extra

sudo ln -sf $HOME/.cargo/bin/nu /usr/local/bin/nu

nu script/installer/install.nu
