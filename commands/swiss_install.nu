
# Update settings of startup with swiss startups 
# Take care this will override startup and prompt config settings
def swiss-startup-reconfigure [] {
    
}

let startup_settings = [    
    "pathvar reset",
    "mkdir ~/.cache/starship", 
    "starship init nu | save ~/.cache/starship/init.nu", 
    "source ~/.cache/starship/init.nu", 
    "zoxide init nushell --hook prompt | save ~/.zoxide.nu", "source ~/.zoxide.nu", 
    "starship init nu | save ~/.cache/starship/init.nu",
    "source ~/.swiss_rc",
    "source ~/.swiss_dyn",
    "source ~/.cache/starship/init.nu",
    "source ~/.zoxide.nu",
]

config set startup $startup_settings 
config set prompt "starship_prompt"

