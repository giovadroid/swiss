# check if file exists
def file-exists [
    path: string # filepath tocheck if exists
] {
   
    let exists = (do -i {
        (ls $path) | ignore
        echo "0";
    })
    let pathfound = ($exists | empty?);
    $pathfound == $false;
}

# Checks if swiss are working
def swiss-check [] {
    echo "swiss are installed!"
}

# Watcher to check if your editing config works
def swiss-watch [
    command?: string # Command to run on watch
] {
    cd $nu.env.SWISS_HOME
    let isEmptyCommand = ($command | empty?)
    let-env no_install_root = 1
    if ($isEmptyCommand == $true) {
        watchexec --shell nu "swiss-check" --watch commands --watch script
    } {
        watchexec --shell nu $command --watch commands --watch script
    }
}