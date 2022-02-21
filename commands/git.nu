# Git push with automatized --set-upstream
def gpush [
    ...args: string     # Extra arguments to add (DEPRECATED)
] {
    bash -c $"git push ($args | str collect ' ')&> /tmp/gpush"
    let upstream = (open /tmp/gpush --raw | lines)
    let upstreamMissing = ($upstream  | where $it =~ "--set-upstream")
    if ( ($upstreamMissing | length) > 1 ) {
         $upstreamMissing | each { nu -c $"($it | str trim)" }
    } {
        echo $upstream
    }
     
}

alias gpull = git pull

