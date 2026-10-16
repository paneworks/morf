-- What the integrations in this directory may touch (PLAN item 4).
-- One manifest covers the directory: beside every integration's entry
-- point, as the loader looks for it. Declared, not yet enforced.
return {
  fs = true,       -- read caches, configs and state under the user's dirs
  run = true,      -- taskwarrior, clipse and friends are programs to run
  http = true,     -- weather, github, lyrics and package lookups
  socket = true,   -- morf.connect for the compositor's socket
  services = true, -- lyrics follows the media service
}
