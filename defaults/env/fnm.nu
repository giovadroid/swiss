
def fnm-init-env [] {
  if (sys).host.name == "Windows" {
    fnm env --shell powershell | lines | str replace '\$env:' '' | str replace -a '"' '' | split column " = " | rename name value | where name != "FNM_ARCH" and name != "PATH" | reduce -f {} {|it, acc| $acc | upsert $it.name $it.value }
  } else {
    fnm env --shell bash | lines | str replace 'export ' '' | str replace -a '"' '' | split column = | rename name value | where name != "PATH" | reduce -f {} {|it, acc| $acc | upsert $it.name $it.value }
  }
}

def fnm-init-path [] {
    let bin = ($env.FNM_MULTISHELL_PATH | path join "bin");
  if (sys).host.name == "Windows" and ($env.Path | where $it =~ $bin | length) == 0 {
    $env.Path | prepend $bin
  } else if (sys).host.name != "Windows" and ($env.PATH | where $it =~ $bin | length) == 0 {
    $env.PATH | prepend $bin
  } else if (sys).host.name == "Windows" {
    $env.Path
  } else {
    $env.PATH
  }
}

load-env (fnm-init-env)
let-env PATH  = (fnm-init-path)
# I´m not sure but in windows can used both
let-env Path = $env.PATH
