local wezterm = require 'wezterm'

local TabBackground = "#262626"
local TabForeground = "#c0b000"
local TabForegroundActive = "#c0b18b"

function string.split(str, sep)
    local t = {}
    for s in string.gmatch(str, "([^" .. sep .. "]+)") do
        table.insert(t, s)
    end
    return t
end

function reduce_title(title)
    title = title:gsub("\\", "/")
    title = title:split("/")
    return title[#title]
end

wezterm.on("format-tab-title", function(tab, tabs, panes, config, hover, max_width)
    return reduce_title(tab.active_pane.title)
end)

wezterm.on("format-window-title", function(tab, pane, tabs, panes, config)
    return reduce_title(tab.active_pane.title)
end)

return {
    font = wezterm.font_with_fallback({
      "FantasqueSansMono Nerd Font", 
      "FiraCode Nerd Font",
      "JetBrainsMono Nerd Font",
    }),                              
    font_size = 13.0,
    -- You can specify some parameters to influence the font selection;
    -- for example, this selects a Bold, Italic font variant.
    font = wezterm.font("FantasqueSansMono Nerd Font", {weight="Bold", italic=false}),    

      -- This causes `wezterm` to act as though it was started as
      -- `wezterm connect unix` by default, connecting to the unix
      -- domain on startup.
      -- If you prefer to connect manually, leave out this line.
    -- default_gui_startup_args = {"connect", "unix"},
    default_prog = {"nu"},
    -- color_scheme = "Whimsy",
    -- color_scheme = "Sublette",
    -- color_scheme = "Gruvbox Dark",
    color_scheme = "Ayu Mirage",
    initial_cols = 100,
    initial_rows = 30,
    default_cursor_style = "BlinkingBlock",
    cursor_blink_rate = 500,
    hide_tab_bar_if_only_one_tab = true,
    window_background_opacity = 1.0,
    alternate_buffer_wheel_scroll_speed = 1,
    window_padding = {
        left = 20,
        right = 20,
        top = 20,   
        bottom = 20
    },
    colors = {
        tab_bar = {
            background = TabBackground,
            active_tab = {
                bg_color = TabBackground,
                fg_color = TabForegroundActive,
                intensity = "Bold"
            },
            inactive_tab = {
                bg_color = TabBackground,
                fg_color = TabForeground,
                intensity = "Normal"
            },
            inactive_tab_hover = {
                bg_color = TabBackground,
                fg_color = TabForegroundActive,
                intensity = "Normal"
            },
            new_tab = {
                bg_color = TabBackground,
                fg_color = TabForeground
            },
            new_tab_hover = {
                bg_color = TabBackground,
                fg_color = TabForegroundActive
            }
        }
    },
    keys = {
        {
            key = "Tab",
            mods = "CTRL",
            action = wezterm.action {
                ActivateTabRelative = 1
            }
        },
        {
            key = "Tab",
            mods = "CTRL|SHIFT",
            action = wezterm.action {
                ActivateTabRelative = -1
            }
        },
        {
            -- This will create a new split and run your default program inside it

            key = "\"",
            mods = "CTRL|SHIFT",
            action = wezterm.action {
                SplitVertical = {
                    domain = "CurrentPaneDomain"
                }
            }
        },
        {
            -- This will create a new split and run the `top` program inside it
            key = "\"",
            mods = "CTRL|SHIFT|ALT",
            action = wezterm.action {
                SplitVertical = {
                    args = {"top"}
                }
            },
        },
        -- This will create a new split and run the `top` program inside it
        {
            key = "%",
            mods = "CTRL|SHIFT",
            action = wezterm.action {
                SplitPane = {
                    direction = "Right",
                    size = {
                        Percent = 50
                    }
                }
            }
        }
    }
}
