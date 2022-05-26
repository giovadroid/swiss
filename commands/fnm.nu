#load env variables
load-env (fnm env --shell bash | lines | str replace 'export ' '' | str replace -a '"' '' | split column = | rename name value | where name != "FNM_ARCH" && name != "PATH" | reduce -f {} {|it, acc| $acc | upsert $it.name $it.value })

#add dynamic fnm path
let-env PATH = $"($env.FNM_MULTISHELL_PATH)/bin:($env.PATH)"'"')

# returns vars to be loaded with load-env
#def fnm-env-nu [] {
#    fnm env --shell bash | lines | where $it !~ ' PATH=' | str replace 'export ' '' | str replace -a '"' '' | split column = | rename name value    
#}
