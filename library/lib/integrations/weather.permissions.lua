-- weather: forecasts from Open-Meteo (wttr.in as a fallback), kept in morf's cache folder.
return {
  http = { "api.open-meteo.com", "geocoding-api.open-meteo.com", "wttr.in" },
  fs = { "$XDG_CACHE_HOME/morf" },
}
