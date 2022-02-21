
# returns vars to be loaded with load-env
def fnm-env-nu [] {
    fnm env --shell bash | lines | where $it !~ ' PATH=' | str find-replace 'export ' '' | str find-replace -a '"' '' | split column = | rename name value
}
