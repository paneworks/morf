-- Starts the clipboard history with the shell, as impasto's shell.qml does:
-- a copy made before the launcher was first opened is still a copy.
require("services.clipboard").start()
