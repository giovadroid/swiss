
source ~/.swiss/commands/swiss_system.nu

if file-exists ~/.cache/starship {} else {
    mkdir ~/.cache/starship
}

starship init nu | save --raw ~/.starship.nu;
zoxide init nushell --hook prompt | save ~/.zoxide.nu;
source ~/.zoxide.nu
source ~/.starship.nu

# nu swiss env
echo "" | save --raw ~/.swiss_dyn
# bat ~/.swiss_dyn
source ~/.swiss/commands/swiss_init.nu

# source ~/.swiss/commands/fnm.nu;
# fnm-env-nu | load-env;
# pathvar add ($nu.env.FNM_MULTISHELL_PATH + /bin);
# source ~/.swiss_dyn

# char newline;