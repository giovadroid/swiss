#!/bin/bash

# Installing homebrew
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# chsh -s /bin/bash
alias readlink=greadlink
echo "alias readlink=greadlink" >> $HOME/.bashrc

curl https://sh.rustup.rs -sSf | sh -s -- -y

source $HOME/.cargo/env

cargo install nu --features=extra 

sudo ln -sf $HOME/.cargo/bin/nu /usr/local/bin/nu

brew install fzf

nu script/installer/install.nu
