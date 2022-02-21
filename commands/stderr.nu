# Run nu in sandbox in order to capture stderr (not concurrency supports)
def stderr [
    ...commands: string     # nu commands
] {
    # To capture stderr will run in bash shell nu script then return stderr and stdout
    let strCommand = ($commands | str collect " ")
    let nuCommand = $"nu -c '($strCommand)'"
    bash -c $"($nuCommand) &> /tmp/stderr"

    let out = (open --raw /tmp/stderr)

    rm /tmp/stderr
    
    return $out
}

