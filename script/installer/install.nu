#!/usr/local/bin/nu

# First step is to save current session env and path to config
config set env $nu.env 
config set path $nu.path

if ( $nu.env.SWISS_HOME == "" ) {
    echo "SWISS_HOME Not exists trying to create"; char newline
    let-env SWISS_HOME = (pwd)
    config set env $nu.env | ignore
} { }

source commands/print.nu;       
source commands/createLink.nu;
source commands/swiss_system.nu;


if (file-exists ~/.swiss) {} {
    createLink $nu.env.SWISS_HOME ~/.swiss -D 1;
}

if (file-exists ~/.swiss_dyn) {} {
    createLink ($nu.env.SWISS_HOME + "/.dyn") ~/.swiss_dyn -D 0;
}

if (file-exists ~/.swiss_rc) {} {
    createLink ($nu.env.SWISS_HOME + "/commands/swiss_rc.nu") ~/.swiss_rc -D 0;
}

let hadLink = ( ls ~ -a | where name =~ .swiss | length ) 

if ( file-exists ~/.swiss ) { } {
    echo "SWISS_HOME not linked into system\n please link swiss into ~/.swiss" $hadLink
    char newline
    exit --now
} 

source ~/.swiss_rc;
source ~/.swiss_dyn;

# Install cross-platform cargo packages

println "Installing cargo tools and starship"
if ((sys).host.name == "Windows") {
    let-env HOME = ($nu.env.USERPROFILE);
    config set env $nu.env | ignore

    echo "In windows uses nightly version, wait until install it";char newline
    # On windows requires nightly for some components
    rustup toolchain install nightly | ignore;
    rustup default nightly | ignore;
    
    # StarShip installtion
    echo "Install Done, now installing starship";char newline
    
    do {
        nu -c "scoop install starship | ignore"
    }
} {
    println "Adding extra components for unix OS";
    char newline;
    cargo install zellij exa rmesg;
    # Not working well czkawka_gui

    # StarShip installation
    # (curl -fsSL https://starship.rs/install.sh | sh -s --  -y );
}

pathvar add ($nu.env.HOME + "/.cargo/bin")  # Added cargo to path in this session

let linkPath = ($nu.env.SWISS_HOME + "/res/links");

if ( file-exists ~/.config/helix ) { } {
    mkdir ~/.config/helix
}

if ( file-exists ~/.config/helix/config.toml ) { } {
    createLink ($linkPath + "/helix.toml" | str collect) ~/.config/helix/config.toml -D 0;
}

if ( file-exists ~/.config/zellij ) { } {
    mkdir ~/.config/zellij
}

if ( file-exists ~/.config/starship.toml ) { } {
    createLink ($linkPath + "/starship") ~/.config/starship.toml -D 0;
}

if ( file-exists ~/.config/zellij/config.yaml ) { } {
    createLink ($linkPath + "/zellij.yaml") ~/.config/zellij/config.yaml -D 0;
}

if ( file-exists ~/.config/helix/runtime ) { } {
    createLink ($nu.env.SWISS_HOME + "/submodules/helix/runtime") ~/.config/helix/runtime -D 1;
}

source commands/swiss_rc.nu;
source ./.dyn;

# config set line_editor "hx" | ignore

# Install common cargo utils
cargo install  cargo-watch cargo-update cargo-edit 

# Install common shell utils
cargo install find-files ripgrep watchexec-cli hexyl zoxide czkawka_cli dua-cli
println Shell tools installation done!

# Install new tools to be tested
cargo install bat procs sd tokei hyperfine bottom tealdeer bandwhich grex git-delta fnm websocat

# post install task 
git config --global core.pager delta
git config --global interactive.diffFilter "delta --color-only --features=interactive"

tldr --update

source commands/swiss_install.nu

# swiss-startup-reconfigure;

if ((sys).host.name == "Windows") {

} {
    let exists = (do -i {
        $nu.env.no_install_root | ignore
        echo "0";
    })
    let runasroot = ($exists | empty?);

    if ($runasroot) { 
        echo  "Running root installation required for unix OS"; char newline;
        sudo -S ($nu.env.HOME + "/.cargo/bin/nu") ((pwd) + "/script/installer/root.nu")
    } { }
}
source commands/print.nu

println WARNING temporally suppresed helix installation 
println WARNING Run this command manually
println git submodule update "--init --recursive" 
println cd submodules/helix
println cargo install "--path" helix-term 
# Install helix
# git submodule update --init --recursive
# cd submodules/helix
# cargo install --path helix-term 
# cd ../../
#gitHelix checkout master
#git submodule update --init --recursive --force
#cargo install --path ($nu.env.SWISS_HOME + "/submodules/helix/helix-term")
println WARNING temporally suppresed rust-analyzer installation 
println WARNING Run this command manually
println git submodule update "--init --recursive" 
println cd submodules/rust-analyzer
println "cargo xtask install --server"

println Installation complete enjoy it!
# add new cargo repos or review    
# https://github.com/svenstaro/genact
# https://github.com/emilk/egui
# https://github.com/copy/v86
# https://github.com/babysor/MockingBird
