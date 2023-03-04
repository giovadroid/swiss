let-env SWISS_HOME = "~/.config/swiss/"
let-env SWISS_VERSION = "0.1.0"
let-env SWISS_USER = "~/.swiss"

swiss init | from yaml | load-env

source ~/.config/swiss/env.dyn.nu;
