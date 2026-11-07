# hypredit

an application to edit your hyprland configs with a ui (inspired from hyprmod!) :3

## what it does

- **live options** — tweak hyprland settings at runtime through `hyprctl`, no restart needed
- **config editor** — append / edit / delete options straight in `hyprland.lua`
- **monitors** — detect your displays, apply per-monitor settings, preview layouts
- **keybinds, rules, autostart, env, cursor, animations** — friendly forms instead of raw text
- **profiles** — save, apply, and flip between config profiles
- **pending & undo** — see what's queued and roll it back
- **console & search** — run commands and hunt down any option
- **wiki** — browse the whole hyprland wiki in-app (live + offline), with type-aware controls and beginner recommendations

## build

```sh
make            # release build
make test       # run tests
make install    # install to /usr/local (PREFIX=... to change)
make uninstall
```

or just cargo:

```sh
cargo build --release
```

## run

```sh
hypredit
hypredit --help
hypredit -c ~/.config/hypr-backup     # use a different config path
hypredit profile list
hypredit profile apply <name>
hypredit profile next
hypredit profile previous
```

logs land in `~/.local/state/hypredit/hypredit.log`.

## packaging

an arch `PKGBUILD` is included:

```sh
makepkg -si
```

## notes

- built with rust + gtk4 + libadwaita, wiki rendered with webkitgtk
- `logo.png` ships as the app icon (hicolor theme)
- licensed under the BSD 2-Clause license, see `LICENSE`
