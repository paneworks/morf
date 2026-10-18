-- tor: starts and stops the tor service with systemctl and reads its progress from the journal.
return {
  run = { "systemctl", "journalctl" },
}
