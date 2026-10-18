-- lyrics: the playing track's lyrics from LRCLIB, kept in morf's cache folder.
return {
  http = { "lrclib.net" },
  fs = { "$XDG_CACHE_HOME/morf" },
  services = { "media" },
}
