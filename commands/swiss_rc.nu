
source ~/.swiss/commands/swiss_system.nu

if (file-exists ~/.cache/starship) {} {
    mkdir ~/.cache/starship
}

starship init nu | save ~/.cache/starship/init.nu
zoxide init nushell --hook prompt | save ~/.zoxide.nu
source ~/.zoxide.nu

# nu swiss env
source ~/.swiss/commands/swiss_init.nu

source ~/.swiss/commands/fnm.nu;
fnm-env-nu | load-env;

pathvar add ($nu.env.FNM_MULTISHELL_PATH + /bin);
char newline;