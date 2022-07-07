local wezterm = require 'wezterm'

local TabBackground = "#000"
local TabForeground = "#aaa"
local TabForegroundActive = "#fff"

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
    default_prog = {"nu.exe"},
    color_scheme = "Hipster Green",
    font = wezterm.font("FiraCode Nerd Font"),
    initial_cols = 100,
    initial_rows = 30,
    font_size = 10,
    default_cursor_style = "BlinkingBar",
    cursor_blink_rate = 500,
    hide_tab_bar_if_only_one_tab = true,
    window_background_opacity = 0.85,
    alternate_buffer_wheel_scroll_speed = 1,
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
            key = "t",
            mods = "CTRL",
            action = wezterm.action {
                SpawnTab = "DefaultDomain"
            }
        },
        {
            key = "w",
            mods = "CTRL",
            action = wezterm.action {
                CloseCurrentTab = {
                    confirm = false
                }
            }
        },
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
            mods = "CTRL|SHIFT|ALT",
            action = wezterm.action {
                SplitVertical = {
                    domain = "CurrentPaneDomain"
                }
            }
        },

        -- This will create a new split and run the `top` program inside it
        key = "\"",
        mods = "CTRL|SHIFT|ALT",
        action = wezterm.action {
            SplitVertical = {
                args = {"top"}
            }
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
