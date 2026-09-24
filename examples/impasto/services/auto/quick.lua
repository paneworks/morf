-- The quick settings' services, started with the shell.
--
-- The libraries under them declare their state as signals, and signals are
-- declared while the configuration loads, so each is connected here rather
-- than on first use. Connecting only reads: nothing here changes the
-- machine until a click asks it to.
for _, name in ipairs {
  "services.network", "services.bluetooth", "services.audio", "services.battery",
  "services.brightness", "services.media", "services.spectrum", "services.system", "services.osd",
  "services.modules", "services.controls",
} do
  require(name)
end
