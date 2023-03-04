# If you open wezterm some paths are missings

let-env PATH = if ($env.PATH | where $it =~ /usr/local/bin | length) == 0 {
  $env.PATH | prepend "/usr/local/bin"
} else {
  $env.PATH
}

let-env PATH = if ($env.PATH | where $it =~ cargo/bin | length) == 0 {
  $env.PATH | prepend "~/.cargo/bin"
} else {
  $env.PATH
}