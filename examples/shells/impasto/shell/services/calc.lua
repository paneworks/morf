-- The launcher's calculator: arithmetic, parsed here, never run as code.
--
-- impasto whitelisted `[0-9+\-*/(). %]` and handed the rest to JavaScript.
-- Nothing typed is ever given to `load` here: a small recursive-descent
-- parser reads numbers, + - * / % ^ (and **), parentheses, unary signs, a
-- handful of functions and constants, and nothing else.
--
--     expr    := term (("+" | "-") term)*
--     term    := unary (("*" | "/" | "%") unary)*
--     unary   := ("+" | "-") unary | power
--     power   := atom (("^" | "**") unary)?        right-associative
--     atom    := number | name | name "(" expr ")" | "(" expr ")"

local M = {}

local FUNCTIONS = {
  sqrt = math.sqrt, abs = math.abs, floor = math.floor, ceil = math.ceil,
  round = function(x) return math.floor(x + 0.5) end,
  sin = math.sin, cos = math.cos, tan = math.tan,
  asin = math.asin, acos = math.acos, atan = math.atan,
  ln = math.log, log = function(x) return math.log(x, 10) end,
  exp = math.exp,
}
local CONSTANTS = { pi = math.pi, e = math.exp(1), tau = 2 * math.pi }

-- Tokens: { kind = "num" | "op" | "name", value }.
local function tokenize(text)
  local tokens, position = {}, 1
  local limit = 256
  if #text > limit then return nil end
  while position <= #text do
    local c = text:sub(position, position)
    if c:match("%s") then
      position = position + 1
    elseif c:match("[%d%.]") then
      local number = text:match("^%d*%.?%d*[eE][%+%-]?%d+", position) or text:match("^%d*%.?%d*", position)
      local value = tonumber(number)
      if not value then return nil end
      tokens[#tokens + 1] = { kind = "num", value = value }
      position = position + #number
    elseif text:sub(position, position + 1) == "**" then
      tokens[#tokens + 1] = { kind = "op", value = "^" }
      position = position + 2
    elseif c:match("[%+%-%*/%%%^%(%)]") then
      tokens[#tokens + 1] = { kind = "op", value = c }
      position = position + 1
    elseif text:sub(position, position + 1) == "×" then
      tokens[#tokens + 1] = { kind = "op", value = "*" }
      position = position + #"×"
    elseif c:match("%a") then
      local name = text:match("^%a+", position)
      tokens[#tokens + 1] = { kind = "name", value = name:lower() }
      position = position + #name
    else
      return nil
    end
  end
  return tokens
end

--- The value of an expression, or nil when it is not arithmetic or has no
--- finite value.
function M.evaluate(text)
  local tokens = tokenize(tostring(text or ""))
  if not tokens or #tokens == 0 then return nil end
  local position = 1
  local depth = 0

  local function peek() return tokens[position] end
  local function take() position = position + 1 return tokens[position - 1] end
  local function is_op(value)
    local token = tokens[position]
    return token and token.kind == "op" and token.value == value
  end

  local expr

  local function atom()
    local token = take()
    if not token then error("end") end
    if token.kind == "num" then return token.value end
    if token.kind == "op" and token.value == "(" then
      depth = depth + 1
      if depth > 64 then error("deep") end
      local value = expr()
      depth = depth - 1
      if not is_op(")") then error("paren") end
      take()
      return value
    end
    if token.kind == "name" then
      local fn = FUNCTIONS[token.value]
      if fn then
        if not is_op("(") then error("call") end
        take()
        depth = depth + 1
        if depth > 64 then error("deep") end
        local argument = expr()
        depth = depth - 1
        if not is_op(")") then error("paren") end
        take()
        return fn(argument)
      end
      local constant = CONSTANTS[token.value]
      if constant then return constant end
    end
    error("token")
  end

  local unary

  local function power()
    local base = atom()
    if is_op("^") then
      take()
      return base ^ unary()
    end
    return base
  end

  unary = function()
    if is_op("-") then take() return -unary() end
    if is_op("+") then take() return unary() end
    return power()
  end

  local function term()
    local value = unary()
    while true do
      local token = peek()
      if not (token and token.kind == "op") then break end
      if token.value == "*" then take() value = value * unary()
      elseif token.value == "/" then take() value = value / unary()
      elseif token.value == "%" then take()
        local divisor = unary()
        -- JavaScript's remainder: the sign of the dividend.
        value = math.fmod(value, divisor)
      else break end
    end
    return value
  end

  expr = function()
    local value = term()
    while true do
      if is_op("+") then take() value = value + term()
      elseif is_op("-") then take() value = value - term()
      else break end
    end
    return value
  end

  local ok, value = pcall(expr)
  if not ok or position <= #tokens then return nil end
  if type(value) ~= "number" or value ~= value or value == math.huge or value == -math.huge then
    return nil
  end
  return value
end

--- As JavaScript would print it: 42, 0.1, 1.5e+21, without float noise.
function M.format(value)
  if value == math.floor(value) and math.abs(value) < 1e15 then
    return ("%d"):format(value)
  end
  local text = ("%.12g"):format(value)
  if text:find("e") then
    local mantissa, exponent = text:match("^(.-)e(.*)$")
    return mantissa .. "e" .. (tonumber(exponent) >= 0 and "+" or "") .. tonumber(exponent)
  end
  return text
end

return M
