-- Sampling policy is shared; graph geometry and paint belong to the theme.
return require("themes").view("graphs").new(require("lib.sysinfo").history_size)
