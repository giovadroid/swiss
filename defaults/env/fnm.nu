
def fnm-init-env [] {
  if (sys).host.name == "Windows" {
    fnm env --shell powershell | lines | str replace '\$env:' '' | str replace -a '"' '' | split column " = " | rename name value | where name != "FNM_ARCH" and name != "PATH" | reduce -f {} {|it, acc| $acc | upsert $it.name $it.value }
  } else {
    fnm env --shell bash | lines | str replace 'export ' '' | str replace -a '"' '' | split column = | rename name value | where name != "PATH" | reduce -f {} {|it, acc| $acc | upsert $it.name $it.value }
  }
}

def fnm-init-path [] {
  if (sys).host.name == "Windows" {
    $env.FNM_MULTISHELL_PATH | prepend $env.Path
  } else {
    $env.FNM_MULTISHELL_PATH | path join "bin" | prepend $env.PATH
  }  
}

load-env (fnm-init-env)
let-env PATH  = (fnm-init-path)
# I´m not sure but in windows can used both
let-env Path = $env.PATH
