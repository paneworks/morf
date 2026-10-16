-- github: a user's contribution calendar from GitHub, kept in morf's cache folder.
return {
  http = { "api.github.com", "github.com" },
  fs = { "$XDG_CACHE_HOME/morf" },
}
