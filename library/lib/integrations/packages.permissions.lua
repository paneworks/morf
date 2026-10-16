-- packages: installed packages and pending updates from pacman and flatpak, and the AUR's versions.
return {
  run = { "pacman", "checkupdates", "flatpak" },
  http = { "aur.archlinux.org" },
}
