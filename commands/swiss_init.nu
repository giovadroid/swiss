# allows you to source string path into ~/.swiss_dyn
def dynSource [
    src: string    # Path to file source
] {
    open $src --raw | save --raw --append ~/.swiss_dyn
    char newline | save --raw --append ~/.swiss_dyn
}

echo | save --raw ~/.swiss_dyn;

ls ($nu.env.SWISS_HOME + "/commands") | where name !~ swiss_ | each { dynSource $it.name }

alias swiss-reload = source ~/.swiss_dyn
alias swiss-install = ~/.swiss/script/installer/install.nu
    
# do update with git resrcs
def swiss-update [] {
    cd $nu.env.SWISS_HOME
    git pull
    nu ($nu.env.SWISS_HOME + "/script/updater/update.nu")
}

# Performs reconfiguration with your default shell
def swiss-reconfigure [] {
    cd $nu.env.SWISS_HOME
    nu "script/installer/install.nu"
    echo "Reconfiguration complete if you still having trouble use .sh or .ps1 instead"; char newline
}

# Print and ads new line
def println [
    ...message: string # Message to be printed Joined with spaces
] {
    echo ($message | str collect " ") ;char newline
}

# Install without stdout cargo packages
def silentInstall [
    ...pkg: string # packages to install
] {
    let command = "cargo install -q"
    let strPkg = ($pkg | str collect " ")
    echo ($command + $strPkg); char newline
    bash -c $"($command + $strPkg)";
}

# Run commands on helix git repo
def gitHelix [
    ...cmds # git args to run into helix repo
] {
    let repoPath = ($nu.env.SWISS_HOME + "/submodules/helix/.git")
    let args = ($cmds | str collect " ")
    let gitCmd = $"git --git-dir ($repoPath) ($args)"
    nu -c $"($gitCmd)"
}

# Show swiss help
def swiss-help [] {
    open ($nu.env.SWISS_HOME + "/README.md")
}