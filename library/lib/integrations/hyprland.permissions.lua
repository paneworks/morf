-- hyprland: the compositor's two sockets; `dispatch("exec", ...)` and `eval` run whatever they are given.
return {
  socket = { "$XDG_RUNTIME_DIR/hypr" },
  run = true,
}
