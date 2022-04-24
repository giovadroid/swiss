
# returns vars to be loaded with load-env
def fnm-env-nu [] {
    fnm env --shell bash | lines | where $it !~ ' PATH=' | str replace 'export ' '' | str replace -a '"' '' | split column = | rename name value    
}
