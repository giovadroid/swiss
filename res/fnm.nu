fnm env --shell bash | lines | where $it !~ ' PATH=' | str find-replace 'export ' '' | str find-replace -a '"' '' | split column = | rename name value
