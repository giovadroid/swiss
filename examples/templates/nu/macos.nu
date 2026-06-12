# If you open wezterm some paths are missings

$env.PATH = if ($env.PATH | where $it =~ /usr/local/bin | length) == 0 {
  $env.PATH | prepend "/usr/local/bin"
} else {
  $env.PATH
}

$env.PATH = if ($env.PATH | where $it =~ cargo/bin | length) == 0 {
  $env.PATH | prepend "~/.cargo/bin"
} else {
  $env.PATH
}