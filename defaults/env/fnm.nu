
def fnm-init-env [] {
  if (sys).host.name == "Windows" {
    fnm env --shell powershell | lines | str replace '\$env:' '' | str replace -a '"' '' | split column " = " | rename name value | where name != "FNM_ARCH" and name != "PATH" | reduce -f {} {|it, acc| $acc | upsert $it.name $it.value }
  } else {
    fnm env --shell bash | lines | str replace 'export ' '' | str replace -a '"' '' | split column = | rename name value | where name != "PATH" | reduce -f {} {|it, acc| $acc | upsert $it.name $it.value }
  }
}

def fnm-init-path [] {
  let fnm_dir = ($env.FNM_DIR | path join "bin" | str replace -a \\ \\);
  if (sys).host.name == "Windows" and ($env.Path | where $it =~ fnm_dir | length) == 0 {
    $env.Path | prepend $env.FNM_MULTISHELL_PATH
  } else if (sys).host.name != "Windows" and ($env.PATH | where $it =~ $env.FNM_DIR | length) == 0 {
    let bin = ($env.FNM_MULTISHELL_PATH | path join "bin");
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
