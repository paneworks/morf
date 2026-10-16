-- The system services `morf.service` binds, by name: the module that
-- defines each and what it holds. Plain data, read by the registry and,
-- as a manifest is, by `morf check` and `morf approve` for the names a
-- module lists under `services`.

return {
  media = { module = "lib.services.mpris", holds = "media players: what plays, and its controls" },
  power = { module = "lib.services.upower", holds = "batteries and power profiles" },
}
