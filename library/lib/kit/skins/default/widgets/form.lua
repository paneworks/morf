-- The default kit's look for the Form widgets, in the Adwaita manner: an
-- error banner tinted with the destructive tone at 9 px corners that
-- slides down over the fields once a send was tried, each field's message
-- in the error ink with its glyph under it, a settings page's quiet status
-- line, and the suggested action that gives its label over to the spinner
-- while it sends. The layout is lib.kit.form's, shared by every theme;
-- what shows when, the archetype's.
local look = require("lib.kit.form").look

return function(S, theme, M)
  local P = theme.P
  local R = theme.radius
  local L = look {
    text = M.text, icon = M.icon, loading = M.loading,
    spring = function() return M.spring(520) end,
    quick = function() return { duration = theme.duration.small, easing = theme.ease.standard } end,
    body = theme.size.normal, small = theme.size.small, weight = 700,
    radius = R.medium,
    tones = {
      error_ground = function() local p = P() return p.error:alpha(p.dark and 0.2 or 0.1) end,
      error_edge = function() local p = P() return p.strong and p.error or p.error:alpha(0.3) end,
      error_ink = function() return P().error_ink end,
      ink = function() return P().ink end,
      ink_dim = function() return P().ink_dim end,
      hover = function() local p = P() return p.ink:alpha(p.wash.hover) end,
      saved = function() return P().success_ink end,
      unsaved = function() return P().accent_ink end,
      status_ground = function() local p = P() return p.ink:alpha(p.dark and 0.07 or 0.05) end,
    },
  }

  local function message() return L.message end

  --- A form: the banner over its fields, a message under each.
  S.Form = { summary = L.summary, message = message }
  S.form = S.Form
  S.login_form = S.Form
  --- A settings page: its status line where a form's banner would be.
  S.settings_form = { summary = L.status, message = message }
  --- One row: a failed send says so on a line under it.
  S.inline_form = { summary = L.failure, message = message }

  --- The submit button: the suggested action, the spinner in its ink while
  --- it sends.
  function S.form_submit(t, spec, node, send)
    return L.submit(S.suggested(t, spec, node, send), t, spec, function() return P().on_accent end)
  end
end
