use std::fs;
use std::path::{Path, PathBuf};

use bmm_lib::local_mod_detection;

const MOD_FOLDER_NAME: &str = "BMM-Compat";
const CONFIG_FILE_NAME: &str = "bmm_compat.cfg";
const MODS_INDEX_FILE_NAME: &str = "mods_index.txt";

const LOVELY_TOML: &str = r#"[manifest]
version = "0.1.0"
dump_lua = true
priority = -100

[[patches]]
[patches.copy]
target = "main.lua"
position = "prepend"
sources = [
  "bmm_compat/bootstrap.lua"
]
"#;

const BOOTSTRAP_LUA: &str = r#"local function log_bootstrap(msg)
  local home = os.getenv("HOME") or ""
  local path = nil
  if home ~= "" then
    path = home .. "/Library/Application Support/Balatro/Mods/lovely/log/bmm_compat_bootstrap.log"
  end
  if not path then
    return
  end
  local ok, f = pcall(io.open, path, "a")
  if ok and f then
    f:write(("[%s] %s\n"):format(os.date("%Y-%m-%d %H:%M:%S"), msg))
    f:close()
  end
end

local function read_cfg()
  if not love or not love.filesystem then
    return nil
  end
  local ok, data = pcall(love.filesystem.read, "bmm_compat.cfg")
  if not ok or type(data) ~= "string" then
    return nil
  end
  local cfg = {}
  for line in data:gmatch("[^\r\n]+") do
    local key, value = line:match("^%s*([%w_]+)%s*=%s*(.+)%s*$")
    if key and value then
      cfg[key] = value
    end
  end
  return cfg
end

local function load_init()
  local cfg = read_cfg()
  if not cfg or not cfg.mods_dir or cfg.mods_dir == "" then
    return nil, "missing mods_dir in bmm_compat.cfg"
  end
  local init_path = cfg.mods_dir .. "/BMM-Compat/bmm_compat/init.lua"
  local loader, err = loadfile(init_path)
  if not loader then
    return nil, err or ("loadfile failed: " .. init_path)
  end
  return pcall(loader)
end

local ok, err = load_init()
if not ok then
  log_bootstrap("failed to load bmm_compat.init: " .. tostring(err))
end
"#;

const INIT_LUA: &str = r#"local function bmm_init_log(msg)
  local home = os.getenv("HOME") or ""
  local path = nil
  if home ~= "" then
    path = home .. "/Library/Application Support/Balatro/Mods/lovely/log/bmm_compat_init.log"
  end
  if love and love.filesystem then
    pcall(love.filesystem.append, "logs/bmm_compat_init.log", msg .. "\n")
  end
  if not path then
    return
  end
  local ok, f = pcall(io.open, path, "a")
  if ok and f then
    f:write(("[%s] %s\n"):format(os.date("%Y-%m-%d %H:%M:%S"), msg))
    f:close()
  end
end

local function bmm_init()
local function read_config()
  if not love or not love.filesystem then
    return { enabled = true }
  end
  local ok, data = pcall(love.filesystem.read, "bmm_compat.cfg")
  if ok and type(data) == "string" then
    local cfg = { enabled = true }
    for line in data:gmatch("[^\r\n]+") do
      local key, value = line:match("^%s*([%w_]+)%s*=%s*(.+)%s*$")
      if key and value then
        if key == "enabled" then
          cfg.enabled = value ~= "false"
        elseif key == "mods_dir" then
          cfg.mods_dir = value
        elseif key == "safe_mode" then
          cfg.safe_mode = value ~= "false"
        end
      end
    end
    return cfg
  end
  return { enabled = true }
end

local cfg = read_config()
if not cfg.enabled then
  return
end
if cfg.safe_mode == nil then
  cfg.safe_mode = true
end

local unpack = table.unpack or unpack
local STATE_FILE = "bmm_compat_state.cfg"

local pending = {}

local function ensure_log_dir()
  if love and love.filesystem then
    pcall(love.filesystem.createDirectory, "logs")
  end
end

local function fallback_log_path()
  local home = os.getenv("HOME") or ""
  local appdata = os.getenv("APPDATA") or ""
  local sep = package.config:sub(1, 1)
  local osname = (jit and jit.os) or ""
  if appdata ~= "" then
    return appdata .. sep .. "Balatro" .. sep .. "logs"
  end
  if osname == "OSX" then
    return home .. sep .. "Library" .. sep .. "Application Support" .. sep .. "Balatro" .. sep .. "logs"
  end
  if home ~= "" then
    return home .. sep .. ".config" .. sep .. "Balatro" .. sep .. "logs"
  end
  return nil
end

local function try_open(path)
  local ok, file = pcall(io.open, path, "a")
  if ok and file then
    file:close()
    return true
  end
  return false
end

local function resolve_log_path()
  if _G.BMM_COMPAT_LOG_PATH then
    return _G.BMM_COMPAT_LOG_PATH
  end
  local filename = ("bmm_compat_%s.log"):format(os.date("%Y-%m-%d_%H-%M-%S"))
  local candidates = {}
  if love and love.filesystem and love.filesystem.getSaveDirectory then
    local ok, base = pcall(love.filesystem.getSaveDirectory)
    if ok and base and base ~= "" then
      table.insert(candidates, base .. "/logs/" .. filename)
    end
  end
  local fallback_dir = fallback_log_path()
  if fallback_dir then
    table.insert(candidates, fallback_dir .. "/" .. filename)
  end
  local home = os.getenv("HOME") or ""
  if home ~= "" then
    table.insert(candidates, home .. "/Library/Application Support/Balatro/Mods/lovely/log/" .. filename)
  end
  for _, path in ipairs(candidates) do
    if try_open(path) then
      _G.BMM_COMPAT_LOG_PATH = path
      return path
    end
  end
  if love and love.filesystem then
    _G.BMM_COMPAT_LOG_PATH = "logs/" .. filename
    return _G.BMM_COMPAT_LOG_PATH
  end
  return nil
end

local function flush_pending()
  if #pending == 0 then
    return
  end
  local path = resolve_log_path()
  if not path then
    return
  end
  local queued = pending
  pending = {}
  for _, line in ipairs(queued) do
    local stamp = os.date("%Y-%m-%d %H:%M:%S")
    local is_abs = path:match("^/") ~= nil
      or path:match("^%a:[/\\]") ~= nil
      or path:match("^\\\\") ~= nil
      or path:match("^//") ~= nil
    if love and love.filesystem and love.filesystem.append and not is_abs then
      pcall(love.filesystem.append, path, ("[%s] %s\n"):format(stamp, line))
    else
      local ok, file = pcall(io.open, path, "a")
      if ok and file then
        file:write(("[%s] %s\n"):format(stamp, line))
        file:close()
      end
    end
  end
end

local function log_line(line)
  if not love or not love.filesystem then
    pending[#pending + 1] = line
    return
  end
  local path = resolve_log_path()
  if not path then
    pending[#pending + 1] = line
    return
  end
  ensure_log_dir()
  local stamp = os.date("%Y-%m-%d %H:%M:%S")
  local is_abs = path:match("^/") ~= nil
    or path:match("^%a:[/\\]") ~= nil
    or path:match("^\\\\") ~= nil
    or path:match("^//") ~= nil
  if love.filesystem.append and not is_abs then
    pcall(love.filesystem.append, path, ("[%s] %s\n"):format(stamp, line))
    return
  end
  local ok, file = pcall(io.open, path, "a")
  if ok and file then
    file:write(("[%s] %s\n"):format(stamp, line))
    file:close()
  end
end

local function read_state()
  if not love or not love.filesystem then
    return { disabled_once = {} }
  end
  local ok, data = pcall(love.filesystem.read, STATE_FILE)
  local state = { disabled_once = {} }
  if ok and type(data) == "string" then
    for line in data:gmatch("[^\r\n]+") do
      local key, value = line:match("^%s*([%w_]+)%s*=%s*(.-)%s*$")
      if key == "disabled_once" and value then
        for item in value:gmatch("([^,]+)") do
          local id = item:gsub("^%s+", ""):gsub("%s+$", "")
          if id ~= "" then
            state.disabled_once[id] = true
          end
        end
      end
    end
  end
  return state
end

local function write_state(state)
  if not love or not love.filesystem then
    return
  end
  local disabled = {}
  for id, _ in pairs(state.disabled_once or {}) do
    disabled[#disabled + 1] = id
  end
  table.sort(disabled)
  local contents = "disabled_once=" .. table.concat(disabled, ",") .. "\n"
  pcall(love.filesystem.write, STATE_FILE, contents)
end

local function trim(s)
  return (s:gsub("^%s+", ""):gsub("%s+$", ""))
end

local state = read_state()
local disabled_once = state.disabled_once or {}
local safe_mode_done = false
local safe_mode_errorhook = false
local safe_mode_fallbackhook = false
local safe_mode_err_handler = nil
local safe_mode_err_original = nil
local safe_mode_err_original_base = nil
local safe_mode_updatehook = false
local safe_mode_handling = false
local safe_mode_bypass = false
local safe_mode_run_original = nil
local safe_mode_crash_mods = {}
local list_recent_mods
local ensure_mod_context_hooks
local find_mod_from_trace
local disable_mod

local function basename(path)
  if type(path) ~= "string" then
    return nil
  end
  local clean = path:gsub("[/\\]+$", "")
  if clean:lower():match("%.lua$") then
    local parent = clean:match("^(.*)[/\\][^/\\]+$")
    if parent then
      return parent:match("([^/\\]+)$") or parent
    end
  end
  return clean:match("([^/\\]+)$") or clean
end

local function identify_mod_id(...)
  local args = { ... }
  for _, v in ipairs(args) do
    if type(v) == "table" then
      local id = v.id or v.mod_id or v.modid or v.name or v.display_name
      if type(id) == "string" and id ~= "" then
        return id
      end
      if type(v.path) == "string" then
        local base = basename(v.path)
        if base and base ~= "" then
          return base
        end
      end
    elseif type(v) == "string" then
      local base = basename(v)
      if base and base ~= "" then
        return base
      end
    end
  end
  return nil
end

local function is_disabled_once(mod_id)
  return mod_id and disabled_once[mod_id] == true
end

local function clear_disabled_once(mod_id)
  if mod_id and disabled_once[mod_id] then
    disabled_once[mod_id] = nil
    write_state({ disabled_once = disabled_once })
  end
end

local function add_disabled_once(mod_id, err)
  if not mod_id or mod_id == "" then
    return
  end
  if not disabled_once[mod_id] then
    log_line(("safe_mode disabled_once mod=%s err=%s"):format(mod_id, tostring(err)))
  end
  disabled_once[mod_id] = true
  write_state({ disabled_once = disabled_once })
end

local function guess_mod_from_crash(msg, trace)
  local text = (tostring(msg) .. "\n" .. tostring(trace or "")):lower()
  if text:find("globals.lua:639", 1, true) and text:find("mp", 1, true) then
    return "Multiplayer"
  end
  return nil
end

local function add_crash_mod(name, suspected)
  if type(name) ~= "string" or name == "" then
    return
  end
  local key = name:lower()
  local status = safe_mode_crash_mods[key]
  if suspected and status ~= "identified" then
    safe_mode_crash_mods[key] = "suspected"
  else
    safe_mode_crash_mods[key] = "identified"
  end
end

local function clear_crash_mods()
  safe_mode_crash_mods = {}
end

local function list_crash_mods()
  local out = {}
  for key, status in pairs(safe_mode_crash_mods) do
    out[#out + 1] = { key = key, status = status }
  end
  table.sort(out, function(a, b)
    return a.key < b.key
  end)
  return out
end

local function handle_safe_mode_crash(msg, trace)
  local trace_text = trace or debug.traceback(tostring(msg), 2)
  local folder = find_mod_from_trace(trace_text)
  if folder and folder:lower() == "bmm-compat" then
    log_line("safe_mode crash_skip_self")
    folder = nil
  end
  local guessed = nil
  if not folder then
    guessed = guess_mod_from_crash(msg, trace_text)
  end
  if not folder and not guessed then
    local recent = list_recent_mods()
    if #recent > 0 then
      log_line(("safe_mode recent_mods=%s"):format(table.concat(recent, ", ")))
    else
      log_line("safe_mode recent_mods_empty")
    end
    for _, name in ipairs(recent) do
      add_crash_mod(name, true)
    end
  end
  if folder then
    add_crash_mod(folder, false)
  elseif guessed then
    add_crash_mod(guessed, true)
  end
  if love and love.window and love.window.showMessageBox then
    local buttons = {}
    local disable_index = nil
    local crash_mods = list_crash_mods()
    if #crash_mods > 0 then
      disable_index = #buttons + 1
      buttons[#buttons + 1] = "Disable Listed Mods"
    end
    local close_index = #buttons + 1
    buttons[#buttons + 1] = "Close Game"
    local continue_index = #buttons + 1
    buttons[#buttons + 1] = "Continue crash"
    local detail = "A mod crash was detected."
    if #crash_mods > 0 then
      local names = {}
      for _, item in ipairs(crash_mods) do
        local label = item.key
        if item.status == "suspected" then
          label = label .. " (suspected)"
        end
        names[#names + 1] = label
      end
      detail = detail .. "\nDetected mods: " .. table.concat(names, ", ")
    else
      detail = detail .. "\nCould not identify the mod from the traceback."
    end
    local ok_box, res = pcall(love.window.showMessageBox, "BMM Compatibility", detail, buttons)
    if ok_box and type(res) == "number" then
      if disable_index and res == disable_index then
        local disabled = {}
        local failed = {}
        for _, item in ipairs(crash_mods) do
          local name = item.key
          log_line(("safe_mode crash_disable mod=%s"):format(tostring(name)))
          if disable_mod(cfg.mods_dir, name) then
            disabled[#disabled + 1] = name
          else
            failed[#failed + 1] = name
          end
        end
        if #disabled > 0 then
          pcall(
            love.window.showMessageBox,
            "BMM Compatibility",
            "Disabled: " .. table.concat(disabled, ", ") .. "\nPlease restart the game.",
            "info"
          )
        end
        if #failed > 0 then
          pcall(
            love.window.showMessageBox,
            "BMM Compatibility",
            "Failed to disable: " .. table.concat(failed, ", ") .. "\nPlease disable them manually.",
            "error"
          )
        end
        if love.event and love.event.quit then
          love.event.quit()
        end
        if os and os.exit then
          pcall(os.exit, 1)
        end
        clear_crash_mods()
        return nil
      end
      if res == close_index then
        if love.event and love.event.quit then
          love.event.quit()
        end
        if os and os.exit then
          pcall(os.exit, 1)
        end
        clear_crash_mods()
        return nil
      end
      if res == continue_index then
        safe_mode_bypass = true
        if love then
          love.errorhandler = safe_mode_err_original_base or safe_mode_err_original
          love.errhand = safe_mode_err_original_base or safe_mode_err_original
          if safe_mode_run_original then
            love.run = safe_mode_run_original
          end
        end
        log_line("safe_mode continue_bypass")
        clear_crash_mods()
        return "continue"
      end
    end
  end
  if folder then
    log_line(("safe_mode crash_disable mod=%s"):format(folder))
    disable_mod(cfg.mods_dir, folder)
  else
    log_line(("safe_mode crash_unidentified err=%s"):format(tostring(msg)))
  end
  if love and love.event and love.event.quit then
    love.event.quit()
  end
  if os and os.exit then
    pcall(os.exit, 1)
  end
  return nil
end

local function ensure_safe_mode_errorhandler()
  if safe_mode_bypass then
    return false
  end
  if not (love and (love.errorhandler or love.errhand)) then
    return false
  end
  local current = love.errorhandler or love.errhand
  if current == safe_mode_err_handler then
    return true
  end
  if safe_mode_err_original_base == nil then
    safe_mode_err_original_base = current
  end
  safe_mode_err_original = current
  safe_mode_err_handler = function(msg)
    if safe_mode_bypass then
      if safe_mode_err_original then
        return safe_mode_err_original(msg)
      end
      return nil
    end
    if safe_mode_handling then
      if safe_mode_err_original then
        return safe_mode_err_original(msg)
      end
      return nil
    end
    safe_mode_handling = true
    log_line(("safe_mode errorhandler invoked err=%s"):format(tostring(msg)))
    local trace = debug.traceback(tostring(msg), 2)
    local ok, result = pcall(handle_safe_mode_crash, msg, trace)
    if not ok then
      log_line(("safe_mode crash_handler_failed err=%s"):format(tostring(result)))
    end
    if ok and result == "continue" then
      safe_mode_handling = false
      local target = safe_mode_err_original_base or safe_mode_err_original
      if target then
        return target(msg)
      end
    end
    if love and love.event and love.event.quit then
      pcall(love.event.quit)
    end
    if os and os.exit then
      pcall(os.exit, 1)
    end
    return nil
  end
  if love.errorhandler then
    love.errorhandler = safe_mode_err_handler
  else
    love.errhand = safe_mode_err_handler
  end
  safe_mode_errorhook = true
  log_line("safe_mode hooked errorhandler")
  return true
end

local function split_alternatives(dep)
  local alts = {}
  for part in dep:gmatch("[^|]+") do
    local clean = trim(part)
    if clean ~= "" then
      table.insert(alts, clean)
    end
  end
  return alts
end

local function dep_id(dep)
  local cleaned = dep:match("^%s*([^%(]+)") or dep
  cleaned = trim(cleaned)
  return cleaned:match("^[%w%-%_%.]+") or cleaned
end

local function gather_dependencies(mod)
  if type(mod) ~= "table" then
    return {}
  end
  if type(mod.dependencies) == "table" then
    return mod.dependencies
  end
  if type(mod.depends) == "table" then
    return mod.depends
  end
  return {}
end

local function extract_constraints(dep)
  local constraints = {}
  for group in dep:gmatch("%b()") do
    local inner = group:sub(2, -2)
    local op, ver = inner:match("^%s*(>=|<=|==|>>|<<)%s*(.+)%s*$")
    if op and ver then
      table.insert(constraints, { op = op, ver = trim(ver) })
    end
  end
  return constraints
end

local function parse_version(ver)
  if type(ver) ~= "string" then
    return nil
  end
  local v = trim(ver)
  if v == "" then
    return nil
  end
  local major, minor, patch, rest = v:match("^(%d+)%.(%d+)%.(%d+)(.*)$")
  if not major then
    major, minor, rest = v:match("^(%d+)%.(%d+)(.*)$")
    if major then
      patch = "0"
    else
      major, rest = v:match("^(%d+)(.*)$")
      if major then
        minor, patch = "0", "0"
      else
        return nil
      end
    end
  end
  return {
    major = tonumber(major) or 0,
    minor = tonumber(minor) or 0,
    patch = tonumber(patch) or 0,
    rev = rest or "",
  }
end

local function compare_rev(a, b)
  if a == b then
    return 0
  end
  local a_pre = a:sub(1, 1) == "~"
  local b_pre = b:sub(1, 1) == "~"
  if a_pre ~= b_pre then
    return a_pre and -1 or 1
  end
  if a == "" then
    return 1
  end
  if b == "" then
    return -1
  end
  if a < b then
    return -1
  end
  if a > b then
    return 1
  end
  return 0
end

local function compare_version(a, b)
  local av = parse_version(a)
  local bv = parse_version(b)
  if not av or not bv then
    return nil
  end
  if av.major ~= bv.major then
    return av.major < bv.major and -1 or 1
  end
  if av.minor ~= bv.minor then
    return av.minor < bv.minor and -1 or 1
  end
  if av.patch ~= bv.patch then
    return av.patch < bv.patch and -1 or 1
  end
  return compare_rev(av.rev, bv.rev)
end

local function compare_version_numeric(a, b)
  local av = parse_version(a)
  local bv = parse_version(b)
  if not av or not bv then
    return nil
  end
  if av.major ~= bv.major then
    return av.major < bv.major and -1 or 1
  end
  if av.minor ~= bv.minor then
    return av.minor < bv.minor and -1 or 1
  end
  if av.patch ~= bv.patch then
    return av.patch < bv.patch and -1 or 1
  end
  return 0
end

local function pick_best_version(current, candidate)
  if type(candidate) ~= "string" or candidate == "" then
    return current
  end
  if type(current) ~= "string" or current == "" then
    return candidate
  end
  local cmp = compare_version_numeric(current, candidate)
  if cmp == nil then
    cmp = compare_version(current, candidate)
  end
  if cmp == nil then
    return current < candidate and candidate or current
  end
  return cmp < 0 and candidate or current
end

local function has_wildcard(ver)
  return type(ver) == "string" and ver:find("%*") ~= nil
end

local function wildcard_match(version, pattern)
  local v = parse_version(version)
  if not v then
    return false
  end
  local nums = { v.major, v.minor, v.patch }
  local idx = 1
  for part in pattern:gmatch("[^%.]+") do
    part = trim(part)
    if part == "*" then
      return true
    end
    local n = tonumber(part)
    if not n then
      return false
    end
    if nums[idx] ~= n then
      return false
    end
    idx = idx + 1
  end
  return true
end

local function version_satisfies(version, op, req)
  if type(version) ~= "string" or type(op) ~= "string" or type(req) ~= "string" then
    return nil
  end
  if has_wildcard(req) then
    if op == "==" then
      return wildcard_match(version, req)
    end
    return nil
  end
  local cmp = compare_version(version, req)
  if cmp == nil then
    return nil
  end
  if op == "==" then
    return cmp == 0
  end
  if op == ">=" then
    return cmp >= 0
  end
  if op == "<=" then
    return cmp <= 0
  end
  if op == ">>" then
    return cmp > 0
  end
  if op == "<<" then
    return cmp < 0
  end
  return nil
end

local function check_constraints(mod_version, constraints)
  if #constraints == 0 then
    return true, false
  end
  local any_unknown = false
  for _, c in ipairs(constraints) do
    local res = version_satisfies(mod_version, c.op, c.ver)
    if res == false then
      return false, false
    end
    if res == nil then
      any_unknown = true
    end
  end
  if any_unknown then
    return true, true
  end
  return true, false
end

local mounted_mods_dir = nil
local mounted_mods_dir_norm = nil
local read_mods_index

local function normalize_sep(path)
  if type(path) ~= "string" then
    return ""
  end
  return path:gsub("\\", "/")
end

local recent_mods = {}
local recent_mod_limit = 6
local recent_mod_window_sec = 120
local mods_index_cache = nil

local function to_mounted_path(path)
  if not mounted_mods_dir_norm or mounted_mods_dir_norm == "" then
    return nil
  end
  local norm = normalize_sep(path)
  if norm:sub(1, #mounted_mods_dir_norm) == mounted_mods_dir_norm then
    local rel = norm:sub(#mounted_mods_dir_norm + 1)
    rel = rel:gsub("^/+", "")
    if rel == "" then
      return "__bmm_mods"
    end
    return "__bmm_mods/" .. rel
  end
  return nil
end

local function list_dir_os(path)
  local sep = package.config:sub(1, 1)
  local quoted = path:gsub('"', '\\"')
  local cmd = nil
  if sep == "\\" then
    cmd = 'dir /b "' .. quoted .. '"'
  else
    cmd = 'ls -1 "' .. quoted .. '"'
  end
  local ok, proc = pcall(io.popen, cmd)
  if not ok or not proc then
    return {}
  end
  local out = {}
  for line in proc:lines() do
    out[#out + 1] = line
  end
  proc:close()
  return out
end

local function list_dir(path)
  if love and love.filesystem then
    if mounted_mods_dir ~= path then
      pcall(love.filesystem.unmount, "__bmm_mods")
      local ok_mount, mount_err = pcall(love.filesystem.mount, path, "__bmm_mods")
      if ok_mount then
        mounted_mods_dir = path
        mounted_mods_dir_norm = normalize_sep(path)
      else
        log_line(("warn mods_dir_mount_failed path=%s err=%s"):format(path, tostring(mount_err)))
      end
    end
    local ok_items, items = pcall(love.filesystem.getDirectoryItems, "__bmm_mods")
    if ok_items and type(items) == "table" then
      if #items > 0 then
        return items
      end
      local os_items = list_dir_os(path)
      if #os_items > 0 then
        log_line(("warn mods_dir_os_fallback path=%s count=%d"):format(path, #os_items))
        return os_items
      end
    else
      log_line(("warn mods_dir_items_failed path=%s err=%s"):format(path, tostring(items)))
    end
  end
  local os_items = list_dir_os(path)
  if #os_items > 0 then
    return os_items
  end
  if cfg and cfg.mods_dir and path == cfg.mods_dir then
    local index_items = read_mods_index(path)
    if #index_items > 0 then
      log_line(("warn mods_dir_index_fallback path=%s count=%d"):format(path, #index_items))
      return index_items
    end
  end
  return {}
end

local function escape_pattern(s)
  return (s:gsub("([%%%^%$%(%)%.%[%]%*%+%-%?])", "%%%1"))
end

local function extract_mod_from_path(path)
  if type(path) ~= "string" or path == "" then
    return nil
  end
  local trimmed = path
  if trimmed:sub(1, 1) == "@" then
    trimmed = trimmed:sub(2)
  end
  local norm = normalize_sep(trimmed)
  local mounted = norm:match("^__bmm_mods/([^/]+)/")
  if mounted then
    return mounted
  end
  if cfg and cfg.mods_dir and cfg.mods_dir ~= "" then
    local mods_norm = normalize_sep(cfg.mods_dir)
    local match = norm:match(escape_pattern(mods_norm) .. "/([^/]+)/")
    if match then
      return match
    end
  end
  local generic = norm:match("/Mods/([^/]+)/")
  if generic then
    return generic
  end
  local rel = norm:match("^([^/]+)/")
  if rel and cfg and cfg.mods_dir and cfg.mods_dir ~= "" then
    if not mods_index_cache then
      mods_index_cache = {}
      local items = read_mods_index(cfg.mods_dir)
      for _, name in ipairs(items) do
        if type(name) == "string" and name ~= "" then
          mods_index_cache[name:lower()] = name
        end
      end
    end
    local key = rel:lower()
    if mods_index_cache[key] then
      return mods_index_cache[key]
    end
  end
  return nil
end

local function record_recent_mod(name, source)
  if type(name) ~= "string" or name == "" then
    return
  end
  local lower = name:lower()
  if lower == "bmm-compat" or lower:find("lovely") then
    return
  end
  local now = os.time()
  recent_mods[#recent_mods + 1] = { name = name, t = now, src = source }
  if #recent_mods > recent_mod_limit then
    table.remove(recent_mods, 1)
  end
end

list_recent_mods = function()
  local out = {}
  local seen = {}
  local now = os.time()
  for i = #recent_mods, 1, -1 do
    local item = recent_mods[i]
    if item and item.name and item.t and (now - item.t) <= recent_mod_window_sec then
      local key = item.name:lower()
      if not seen[key] then
        out[#out + 1] = item.name
        seen[key] = true
      end
      if #out >= recent_mod_limit then
        break
      end
    end
  end
  return out
end

find_mod_from_trace = function(trace)
  if type(trace) ~= "string" or trace == "" then
    return nil
  end
  local norm = normalize_sep(trace)
  if cfg and cfg.mods_dir and cfg.mods_dir ~= "" then
    local mods_norm = normalize_sep(cfg.mods_dir)
    local match = norm:match(escape_pattern(mods_norm) .. "/([^/]+)/")
    if match then
      return match
    end
  end
  local generic = norm:match("/Mods/([^/]+)/")
  if generic then
    return generic
  end
  return nil
end

local mod_context_hooked = false
ensure_mod_context_hooks = function()
  if mod_context_hooked then
    return
  end
  mod_context_hooked = true

  local orig_loadfile = loadfile
  if type(orig_loadfile) == "function" then
    loadfile = function(filename, ...)
      local mod = extract_mod_from_path(filename)
      if mod then
        record_recent_mod(mod, "loadfile")
      end
      return orig_loadfile(filename, ...)
    end
  end

  local orig_dofile = dofile
  if type(orig_dofile) == "function" then
    dofile = function(filename, ...)
      local mod = extract_mod_from_path(filename)
      if mod then
        record_recent_mod(mod, "dofile")
      end
      return orig_dofile(filename, ...)
    end
  end

  local orig_require = require
  if type(orig_require) == "function" then
    require = function(name, ...)
      local resolved = nil
      if type(name) == "string" and package and type(package.searchpath) == "function" then
        resolved = package.searchpath(name, package.path)
      end
      local mod = extract_mod_from_path(resolved or name)
      if mod then
        record_recent_mod(mod, "require")
      end
      return orig_require(name, ...)
    end
  end

  local orig_load = load
  if type(orig_load) == "function" then
    load = function(chunk, chunkname, ...)
      local mod = extract_mod_from_path(chunkname)
      if not mod and type(chunk) == "string" then
        mod = extract_mod_from_path(chunk)
      end
      if mod then
        record_recent_mod(mod, "load")
      end
      return orig_load(chunk, chunkname, ...)
    end
  end

  local orig_loadstring = loadstring
  if type(orig_loadstring) == "function" then
    loadstring = function(chunk, chunkname, ...)
      local mod = extract_mod_from_path(chunkname)
      if not mod and type(chunk) == "string" then
        mod = extract_mod_from_path(chunk)
      end
      if mod then
        record_recent_mod(mod, "loadstring")
      end
      return orig_loadstring(chunk, chunkname, ...)
    end
  end

  if love and love.filesystem then
    local orig_fs_load = love.filesystem.load
    if type(orig_fs_load) == "function" then
      love.filesystem.load = function(filename, ...)
        local mod = extract_mod_from_path(filename)
        if mod then
          record_recent_mod(mod, "fs_load")
        end
        return orig_fs_load(filename, ...)
      end
    end

    local orig_fs_read = love.filesystem.read
    if type(orig_fs_read) == "function" then
      love.filesystem.read = function(filename, ...)
        local mod = extract_mod_from_path(filename)
        if mod then
          record_recent_mod(mod, "fs_read")
        end
        return orig_fs_read(filename, ...)
      end
    end
  end
end

ensure_mod_context_hooks()

local function list_lua_candidates(path)
  if love and love.filesystem then
    local mounted = to_mounted_path(path)
    if mounted then
      local out = {}
      local function walk(dir, depth)
        if depth > 3 then
          return
        end
        local ok_items, items = pcall(love.filesystem.getDirectoryItems, dir)
        if not ok_items or type(items) ~= "table" then
          return
        end
        for _, item in ipairs(items) do
          local full = dir .. "/" .. item
          local ok_info, info = pcall(love.filesystem.getInfo, full)
          if ok_info and info then
            if info.type == "file" then
              if item:lower():match("%.lua$") then
                out[#out + 1] = full
              end
            elseif info.type == "directory" then
              walk(full, depth + 1)
            end
          end
        end
      end
      walk(mounted, 0)
      if #out > 0 then
        return out
      end
    end
  end
  local sep = package.config:sub(1, 1)
  local quoted = path:gsub('"', '\\"')
  local cmd = nil
  if sep == "\\" then
    cmd = 'dir /s /b "' .. quoted .. '\\*.lua"'
  else
    cmd = 'find "' .. quoted .. '" -maxdepth 3 -name "*.lua"'
  end
  local ok, proc = pcall(io.popen, cmd)
  if not ok or not proc then
    return {}
  end
  local out = {}
  for line in proc:lines() do
    out[#out + 1] = line
  end
  proc:close()
  return out
end

local function file_exists(path)
  if love and love.filesystem then
    local mounted = to_mounted_path(path)
    if mounted then
      local ok_info, info = pcall(love.filesystem.getInfo, mounted)
      if ok_info and info then
        return true
      end
    end
  end
  local ok, fh = pcall(io.open, path, "r")
  if ok and fh then
    fh:close()
    return true
  end
  return false
end

local function read_file(path)
  if love and love.filesystem then
    local mounted = to_mounted_path(path)
    if mounted then
      local ok_read, data = pcall(love.filesystem.read, mounted)
      if ok_read and type(data) == "string" then
        return data
      end
    end
  end
  local ok, fh = pcall(io.open, path, "r")
  if ok and fh then
    local data = fh:read("*a") or ""
    fh:close()
    return data
  end
  return nil
end

read_mods_index = function(mods_dir)
  local out = {}
  if love and love.filesystem and mounted_mods_dir == mods_dir then
    local ok_read, data = pcall(love.filesystem.read, "__bmm_mods/BMM-Compat/mods_index.txt")
    if ok_read and type(data) == "string" and data ~= "" then
      for line in data:gmatch("[^\r\n]+") do
        local clean = trim(line)
        if clean ~= "" then
          out[#out + 1] = clean
        end
      end
      return out
    end
  end
  local index_path = mods_dir .. "/BMM-Compat/" .. "mods_index.txt"
  local ok, fh = pcall(io.open, index_path, "r")
  if not ok or not fh then
    return {}
  end
  for line in fh:lines() do
    local clean = trim(line)
    if clean ~= "" then
      out[#out + 1] = clean
    end
  end
  fh:close()
  return out
end

local function is_mod_enabled(mods_dir, name)
  if type(mods_dir) ~= "string" or mods_dir == "" or type(name) ~= "string" then
    return true
  end
  local ignore_path = mods_dir .. "/" .. name .. "/.lovelyignore"
  return not file_exists(ignore_path)
end

local function has_multiplayer_fallback(mods_dir)
  if not is_mod_enabled(mods_dir, "Multiplayer") then
    return false
  end
  local base = mods_dir .. "/Multiplayer"
  local candidates = {
    base .. "/ui/smods.lua",
    base .. "/Multiplayer.lua",
    base .. "/Multiplayer.json",
    base .. "/mod.lua",
    base .. "/lovely.toml",
    base .. "/smods.json",
  }
  for _, path in ipairs(candidates) do
    if file_exists(path) then
      return true
    end
  end
  return false
end

local function has_multiplayer_installed(mods_dir)
  if type(mods_dir) ~= "string" or mods_dir == "" then
    return false, nil
  end
  local items = list_dir(mods_dir)
  for _, name in ipairs(items) do
    if type(name) == "string" and is_mod_enabled(mods_dir, name) then
      if name:lower():find("multiplayer") then
        return true, name
      end
    end
  end
  if has_multiplayer_fallback(mods_dir) then
    return true, "Multiplayer"
  end
  return false, nil
end

local function mod_looks_smods(mod_path)
  if type(mod_path) ~= "string" or mod_path == "" then
    return false
  end
  local candidates = {
    mod_path .. "/smods.json",
    mod_path .. "/steamodded.lua",
    mod_path .. "/steamodded_metadata.lua",
    mod_path .. "/ui/smods.lua",
  }
  for _, path in ipairs(candidates) do
    if file_exists(path) then
      return true
    end
  end
  return false
end

local function mod_uses_smods(mod_path)
  if mod_looks_smods(mod_path) then
    return true
  end
  local files = list_lua_candidates(mod_path)
  for _, file in ipairs(files) do
    local data = read_file(file)
    if type(data) == "string" then
      local head = data:sub(1, 8000)
      if head:find("SMODS") then
        return true
      end
    end
  end
  return false
end

local function parse_legacy_deps(mod_path)
  local deps = {}
  local mod_id = nil
  local files = list_lua_candidates(mod_path)
  for _, file in ipairs(files) do
    local data = read_file(file)
    if type(data) == "string" then
      local i = 0
      for line in data:gmatch("[^\r\n]+") do
        i = i + 1
        if not mod_id then
          mod_id = line:match("^%s*%-%-%-%s*MOD_ID:%s*([%w%-%_%.]+)")
        end
        local dep_line = line:match("^%s*%-%-%-%s*DEPENDENCIES:%s*(.+)")
        if dep_line then
          local inside = dep_line:match("%[(.*)%]") or dep_line
          for part in inside:gmatch("([^,]+)") do
            local clean = trim(part)
            if clean ~= "" then
              deps[#deps + 1] = clean
            end
          end
          return deps, mod_id
        end
        if i >= 120 then
          break
        end
      end
    end
  end
  return deps, mod_id
end

local function read_mod_meta(mod_path, mod_name)
  local candidates = {
    mod_path .. "/smods.json",
    mod_path .. "/" .. mod_name .. ".json",
    mod_path .. "/mod.json",
    mod_path .. "/manifest.json",
  }
  for _, path in ipairs(candidates) do
    if file_exists(path) then
      local data = read_file(path)
      if type(data) == "string" then
        local mod_id = data:match('"id"%s*:%s*"([^"]+)"') or mod_name
        local deps = {}
        local deps_block = data:match('"dependencies"%s*:%s*%[(.-)%]')
        if deps_block then
          for dep in deps_block:gmatch('"([^"]+)"') do
            deps[#deps + 1] = dep
          end
        end
        return {
          id = mod_id,
          deps = deps,
        }
      end
    end
  end
  return nil
end

local function collect_fs_issues()
  local issues = {}
  local mods_dir = cfg.mods_dir
  if type(mods_dir) ~= "string" or mods_dir == "" then
    return issues
  end
  local items = list_dir(mods_dir)
  if #items == 0 then
    log_line(("warn mods_dir_empty path=%s"):format(mods_dir))
  end
  local smods_version = nil
  local smods_unknown = false
  local has_multiplayer = false
  local multiplayer_require_checked = false
  local smods_mods = {}
  if SMODS and type(SMODS.version) == "string" then
    smods_version = SMODS.version
  end
  for _, name in ipairs(items) do
    if type(name) == "string" then
      local lower = name:lower()
      local mod_path = mods_dir .. "/" .. name
      if not is_mod_enabled(mods_dir, name) then
        -- skip disabled mods
      elseif lower:find("lovely") or lower == "bmm-compat" then
        -- skip
      elseif lower:find("smods") or lower:find("steamodded") then
        local v = name:match("smods%-([%w%.%-%_~]+)") or name:match("steamodded%-([%w%.%-%_~]+)")
        if v then
          smods_version = pick_best_version(smods_version, v)
          log_line(("info steamodded_detected folder=%s version=%s"):format(name, v))
        else
          -- Steamodded folder found but no version in folder name
          -- Still count as detected to avoid false "not detected" errors
          if not smods_version then
            smods_version = "unknown"
          end
          smods_unknown = true
          log_line(("info steamodded_detected folder=%s version=unknown"):format(name))
        end
      else
        if lower:find("multiplayer") then
          has_multiplayer = true
        end
        if mod_uses_smods(mod_path) then
          smods_mods[#smods_mods + 1] = name
        end
      end
    end
  end
  if not has_multiplayer and has_multiplayer_fallback(mods_dir) then
    has_multiplayer = true
    log_line("warn multiplayer_detected_fallback")
  end
  if not smods_version and #smods_mods > 0 then
    for _, mod in ipairs(smods_mods) do
      issues[#issues + 1] = ("Steamodded not detected for %s (SMODS mod)"):format(mod)
    end
  end
  for _, name in ipairs(items) do
    if type(name) == "string" then
      local lower = name:lower()
      if lower:find("lovely") or lower == "bmm-compat" then
        -- skip
      elseif not is_mod_enabled(mods_dir, name) then
        -- skip disabled mods
      else
        local mod_path = mods_dir .. "/" .. name
        if not smods_version and mod_looks_smods(mod_path) then
          issues[#issues + 1] = ("Steamodded not detected for %s (SMODS mod)"):format(name)
        end
        local meta = read_mod_meta(mod_path, name)
        if meta and meta.deps and #meta.deps > 0 then
          if (meta.id or name):lower() == "multiplayer" then
            multiplayer_require_checked = true
          end
          for _, dep in ipairs(meta.deps) do
            if type(dep) == "string" and dep:lower():find("steamodded") then
              local constraints = extract_constraints(dep)
              if smods_version and smods_version ~= "unknown" then
                local okv, unk = check_constraints(smods_version, constraints)
                if okv == false then
                  issues[#issues + 1] = ("Steamodded version mismatch for %s: %s (have %s)"):format(
                    meta.id or name,
                    dep,
                    smods_version
                  )
                elseif unk then
                  -- Log warning but don't add as blocking issue
                  log_line(("warn version_check_skipped mod=%s dep=%s have=%s"):format(
                    meta.id or name,
                    dep,
                    smods_version
                  ))
                end
              elseif not smods_version then
                issues[#issues + 1] = ("Steamodded version unknown for %s: %s"):format(
                  meta.id or name,
                  dep
                )
              end
              -- When smods_version == "unknown", we skip version checks silently
              end
            end
          end
        else
          local deps, legacy_id = parse_legacy_deps(mod_path)
          local mod_id = legacy_id or name
          for _, dep in ipairs(deps) do
            if type(dep) == "string" and dep:lower():find("steamodded") then
              local constraints = extract_constraints(dep)
              if smods_version and smods_version ~= "unknown" then
                local okv, unk = check_constraints(smods_version, constraints)
                if okv == false then
                  issues[#issues + 1] = ("Steamodded version mismatch for %s: %s (have %s)"):format(
                    mod_id,
                    dep,
                    smods_version
                  )
                elseif unk then
                  -- Log warning but don't add as blocking issue
                  log_line(("warn version_check_skipped mod=%s dep=%s have=%s"):format(
                    mod_id,
                    dep,
                    smods_version
                  ))
                end
              elseif not smods_version then
                issues[#issues + 1] = ("Steamodded version unknown for %s: %s"):format(
                  mod_id,
                  dep
                )
              end
              -- When smods_version == "unknown", we skip version checks silently
              end
            end
          end
        end
      end
    end
  end
  if has_multiplayer and not multiplayer_require_checked then
    if smods_version and smods_version ~= "unknown" then
      local cmp = compare_version_numeric(smods_version, "1.0.0")
      if cmp and cmp < 0 then
        issues[#issues + 1] = ("Steamodded %s is too old for Multiplayer (requires >=1.0.0)"):format(
          smods_version
        )
      end
    elseif not smods_version then
      log_line("warn multiplayer_version_deferred")
    end
    -- When smods_version == "unknown", we skip the version check silently
  end
  return issues
end

local function normalize_path(path)
  if type(path) ~= "string" then
    return ""
  end
  local out = path
  out = out:gsub("%[\"(.-)\"%]", ".%1")
  out = out:gsub("%['(.-)'%]", ".%1")
  out = out:gsub("^%.*", "")
  return out
end

local function get_path_value(path)
  local cur = _G
  local norm = normalize_path(path)
  for part in norm:gmatch("[^%.]+") do
    if type(cur) ~= "table" then
      return nil
    end
    cur = cur[part]
    if cur == nil then
      return nil
    end
  end
  return cur
end

local function audit_api_expectations(mod_id, mod)
  if type(mod) ~= "table" then
    return
  end
  local compat = mod.compat
  if type(compat) ~= "table" then
    return
  end
  local expect = compat.expect
  if type(expect) ~= "table" then
    return
  end
  for _, entry in ipairs(expect) do
    local path = nil
    local expected_type = nil
    if type(entry) == "string" then
      path = entry
    elseif type(entry) == "table" then
      path = entry.path
      expected_type = entry.type
    end
    if path and path ~= "" then
      local value = get_path_value(path)
      if value == nil then
        log_line(("warn api_missing mod=%s path=%s"):format(mod_id, path))
      elseif expected_type and type(value) ~= expected_type then
        log_line(("warn api_type_mismatch mod=%s path=%s expected=%s actual=%s"):format(
          mod_id,
          path,
          expected_type,
          type(value)
        ))
      end
    end
  end
end

local fatal_triggered = false

local function write_ignore(path)
  local ok, file = pcall(io.open, path, "w")
  if ok and file then
    file:write("")
    file:close()
    return true
  end
  return false
end

local function build_mod_id_map(mods_dir)
  local map = {}
  if type(mods_dir) ~= "string" or mods_dir == "" then
    return map
  end
  local items = list_dir(mods_dir)
  for _, name in ipairs(items) do
    if type(name) == "string" then
      map[name:lower()] = name
      local meta = read_mod_meta(mods_dir .. "/" .. name, name)
      if meta and meta.id then
        map[meta.id:lower()] = name
      end
    end
  end
  return map
end

local function list_expected_mods(mods_dir)
  local expected = {}
  if type(mods_dir) ~= "string" or mods_dir == "" then
    return expected
  end
  local items = list_dir(mods_dir)
  for _, name in ipairs(items) do
    if type(name) == "string" then
      local lower = name:lower()
      if lower:find("lovely") or lower == "bmm-compat" then
        -- skip
      elseif lower:find("smods") or lower:find("steamodded") then
        -- skip
      elseif not is_mod_enabled(mods_dir, name) then
        -- skip disabled mods
      else
        local mod_path = mods_dir .. "/" .. name
        local meta = read_mod_meta(mod_path, name)
        if meta and meta.id then
          expected[#expected + 1] = { id = meta.id, folder = name }
        else
          local deps, legacy_id = parse_legacy_deps(mod_path)
          if legacy_id or #deps > 0 then
            expected[#expected + 1] = { id = legacy_id or name, folder = name }
          elseif mod_uses_smods(mod_path) then
            expected[#expected + 1] = { id = name, folder = name }
          end
        end
      end
    end
  end
  return expected
end

local function extract_mods_from_issues(issues)
  local mods = {}
  local function add(name)
    if type(name) ~= "string" then
      return
    end
    local clean = trim(name)
    if clean ~= "" then
      mods[clean:lower()] = clean
    end
  end
  for _, issue in ipairs(issues) do
    if type(issue) == "string" then
      local lower = issue:lower()
      if lower:find("multiplayer") then
        add("Multiplayer")
      end
      local mod = issue:match("for ([^:]+):")
      add(mod)
      mod = issue:match("for ([^%(]+)%s*%(")
      add(mod)
      mod = issue:match("too old for ([^%(]+)")
      add(mod)
      mod = issue:match("version unknown for ([^%(]+)")
      add(mod)
      mod = issue:match("not detected for ([^%(]+)")
      add(mod)
      mod = issue:match("mismatch for ([^:]+):")
      add(mod)
      local missing = issue:match("^Mod ([^ ]+) is installed but was not loaded")
      add(missing)
    end
  end
  local out = {}
  for _, mod in pairs(mods) do
    out[#out + 1] = mod
  end
  table.sort(out)
  return out
end

disable_mod = function(mods_dir, name)
  if type(mods_dir) ~= "string" or mods_dir == "" or type(name) ~= "string" then
    return false
  end
  local mod_path = mods_dir .. "/" .. name
  local ok = write_ignore(mod_path .. "/.lovelyignore")
  if ok then
    log_line(("disabled mod=%s via compat helper"):format(name))
  end
  return ok
end

local function fail_with_issues(issues)
  if #issues == 0 then
    return
  end
  if fatal_triggered then
    return
  end
  fatal_triggered = true
  local msg = "BMM compatibility checks found issues:\n\n"
  local limit = math.min(8, #issues)
  for i = 1, limit do
    msg = msg .. "- " .. issues[i] .. "\n"
  end
  if #issues > limit then
    msg = msg .. string.format("...and %d more.\n", #issues - limit)
  end
  msg = msg .. "\nPlease update Steamodded or disable incompatible mods."
  local mods_to_disable = extract_mods_from_issues(issues)
  local id_map = build_mod_id_map(cfg.mods_dir)
  local resolved = {}
  for _, mod in ipairs(mods_to_disable) do
    local key = mod:lower()
    local folder = id_map[key]
    if folder then
      resolved[#resolved + 1] = folder
    end
  end
  if #resolved == 0 and is_mod_enabled(cfg.mods_dir, "Multiplayer") then
    for _, mod in ipairs(mods_to_disable) do
      if mod:lower() == "multiplayer" then
        resolved[#resolved + 1] = "Multiplayer"
        break
      end
    end
  end
  local can_disable = #resolved > 0
  log_line("fatal " .. msg:gsub("\n", " | "))
  if love and love.window and love.window.showMessageBox then
    if can_disable then
      local ok_box, res = pcall(love.window.showMessageBox, "BMM Compatibility", msg, {
        "Disable Incompatible Mods",
        "OK",
      })
      if ok_box and type(res) == "number" then
        if res == 1 then
          local disabled = {}
          for _, folder in ipairs(resolved) do
            if disable_mod(cfg.mods_dir, folder) then
              disabled[#disabled + 1] = folder
            end
          end
          if #disabled > 0 then
            pcall(
              love.window.showMessageBox,
              "BMM Compatibility",
              "Disabled: " .. table.concat(disabled, ", ") .. "\nPlease restart the game.",
              "info"
            )
          else
            pcall(
              love.window.showMessageBox,
              "BMM Compatibility",
              "Failed to disable incompatible mods automatically. Please disable them manually.",
              "error"
            )
          end
        end
      else
        pcall(love.window.showMessageBox, "BMM Compatibility", msg, "error")
      end
    else
      pcall(love.window.showMessageBox, "BMM Compatibility", msg, "error")
    end
    if love.event and love.event.quit then
      love.event.quit()
    end
  end
  if os and os.exit then
    pcall(os.exit, 1)
  end
  return
end

local function setup_safe_mode()
  if safe_mode_done or safe_mode_bypass or not cfg.safe_mode then
    return
  end
  ensure_mod_context_hooks()
  local function setup_safe_mode_fallback()
    if safe_mode_fallbackhook or not (love and type(love.run) == "function") then
      return false
    end
    local original = love.run
    safe_mode_run_original = original
    love.run = function(...)
      local args = { ... }
      local ok, results = xpcall(function()
        return { original(unpack(args)) }
      end, function(err)
        if safe_mode_bypass then
          return debug.traceback(tostring(err), 2)
        end
        if safe_mode_handling then
          return debug.traceback(tostring(err), 2)
        end
        safe_mode_handling = true
        local trace = debug.traceback(tostring(err), 2)
        local ok_handle, result = pcall(handle_safe_mode_crash, err, trace)
        if not ok_handle then
          log_line(("safe_mode crash_handler_failed err=%s"):format(tostring(result)))
        end
        if ok_handle and result == "continue" then
          safe_mode_handling = false
          return trace
        end
        if love and love.event and love.event.quit then
          pcall(love.event.quit)
        end
        if os and os.exit then
          pcall(os.exit, 1)
        end
        return trace
      end)
      if ok then
        return unpack(results)
      end
      return nil
    end
    safe_mode_fallbackhook = true
    log_line("safe_mode fallback_hooked")
    return true
  end

  local any = false
  any = ensure_safe_mode_errorhandler() or any
  any = setup_safe_mode_fallback() or any
  if not (SMODS and type(SMODS) == "table") then
    if not any then
      log_line("warn safe_mode_no_hook")
    else
      safe_mode_done = true
    end
    return
  end

  local function wrap_smods_fn(name)
    local original = SMODS[name]
    if type(original) ~= "function" then
      return false
    end
    SMODS[name] = function(...)
      local args = { ... }
      local mod_id = identify_mod_id(unpack(args))
      if is_disabled_once(mod_id) then
        log_line(("safe_mode skip mod=%s hook=%s"):format(mod_id, name))
        clear_disabled_once(mod_id)
        return nil
      end
      local ok, results = xpcall(function()
        return { original(unpack(args)) }
      end, debug.traceback)
      if ok then
        return unpack(results)
      end
      local err = results
      if mod_id then
        add_disabled_once(mod_id, err)
      else
        log_line(("safe_mode error hook=%s err=%s"):format(name, tostring(err)))
      end
      return nil
    end
    log_line(("safe_mode hooked %s"):format(name))
    return true
  end

  any = wrap_smods_fn("load_mod") or any
  any = wrap_smods_fn("load_mods") or any
  any = wrap_smods_fn("register_mod") or any
  if not any then
    log_line("warn safe_mode_no_hook")
    return
  end
  safe_mode_done = true
end

local function audit_smods()
  if not (SMODS and SMODS.Mods) then
    log_line("SMODS not detected at init.")
    return false
  end
  log_line("SMODS detected at init.")
  setup_safe_mode()
  if type(SMODS.register_mod) ~= "function" then
    log_line("warn api_missing path=SMODS.register_mod")
    return false
  end

  local issues = collect_fs_issues()

  local have = {}
  for id, mod in pairs(SMODS.Mods) do
    if type(id) == "string" then
      have[id] = mod
      if type(mod) == "table" and type(mod.version) ~= "string" then
        log_line(("warn missing_version mod=%s"):format(id))
      end
      audit_api_expectations(id, mod)
    end
  end
  local have_lower = {}
  for id, _ in pairs(have) do
    have_lower[id:lower()] = true
  end

  for id, mod in pairs(SMODS.Mods) do
    local deps = gather_dependencies(mod)
    for _, dep in ipairs(deps) do
      if type(dep) == "string" then
        local alts = split_alternatives(dep)
        local satisfied = false
        local checked_any = false
        local unknown = false
        for _, alt in ipairs(alts) do
          local depid = dep_id(alt)
          local target = have[depid]
          if target then
            local constraints = extract_constraints(alt)
            local ok, is_unknown = check_constraints(target.version, constraints)
            if is_unknown then
              unknown = true
            end
            checked_any = checked_any or #constraints > 0
            if ok then
              satisfied = true
              break
            end
          end
        end
        if not satisfied then
          log_line(("warn missing_dependency mod=%s dep=%s"):format(id, dep))
          issues[#issues + 1] = ("Missing dependency for %s: %s"):format(id, dep)
        elseif unknown and checked_any then
          log_line(("warn version_check_skipped mod=%s dep=%s"):format(id, dep))
        end
      end
    end
  end

  for id, mod in pairs(SMODS.Mods) do
    local deps = gather_dependencies(mod)
    for _, dep in ipairs(deps) do
      if type(dep) == "string" then
        local alts = split_alternatives(dep)
        for _, alt in ipairs(alts) do
          local depid = dep_id(alt)
          local target = have[depid]
          if target and type(target) == "table" then
            local constraints = extract_constraints(alt)
            if #constraints > 0 then
              local ok, is_unknown = check_constraints(target.version, constraints)
              if ok == false then
                log_line(("warn version_mismatch mod=%s dep=%s have=%s"):format(
                  id,
                  dep,
                  tostring(target.version)
                ))
                issues[#issues + 1] = ("Version mismatch for %s: %s (have %s)"):format(
                  id,
                  dep,
                  tostring(target.version)
                )
              elseif is_unknown then
                log_line(("warn version_check_skipped mod=%s dep=%s"):format(id, dep))
              end
            end
          end
        end
      end
    end
  end

  local steamodded = have["Steamodded"]
  if steamodded and type(steamodded.version) ~= "string" then
    log_line("warn steamodded_version_missing")
  end

  local expected = list_expected_mods(cfg.mods_dir)
  for _, entry in ipairs(expected) do
    if entry.id and not have_lower[entry.id:lower()] then
      log_line(("warn mod_not_loaded id=%s folder=%s"):format(entry.id, tostring(entry.folder)))
      issues[#issues + 1] = ("Mod %s is installed but was not loaded by Steamodded."):format(
        entry.id
      )
    end
  end

  fail_with_issues(issues)
  return true
end

log_line("Compatibility helper enabled.")
local audit_done = false
local preflight_done = false

local function maybe_audit()
  flush_pending()
  if cfg.safe_mode and not safe_mode_bypass then
    ensure_safe_mode_errorhandler()
  end
  setup_safe_mode()
  if audit_done then
    return
  end
  if SMODS and SMODS.Mods then
    local ok_audit = audit_smods()
    if ok_audit then
      audit_done = true
    end
    flush_pending()
  end
end

local function preflight()
  if preflight_done then
    return
  end
  preflight_done = true
  local issues = collect_fs_issues()
  if #issues > 0 then
    fail_with_issues(issues)
  end
end

preflight()
maybe_audit()
if not audit_done and love and love.update then
  local prev_update = love.update
  love.update = function(...)
    maybe_audit()
    if cfg.safe_mode and not safe_mode_bypass then
      ensure_safe_mode_errorhandler()
    end
    return prev_update(...)
  end
  safe_mode_updatehook = true
end

if debug and debug.sethook and (not audit_done or cfg.safe_mode) then
  local hook_ticks = 0
  debug.sethook(function()
    maybe_audit()
    if cfg.safe_mode and not safe_mode_updatehook and not safe_mode_bypass then
      ensure_safe_mode_errorhandler()
    end
    hook_ticks = hook_ticks + 1
    if audit_done and (not cfg.safe_mode or safe_mode_updatehook or hook_ticks > 200) then
      debug.sethook()
    end
  end, "", 10000)
end

if cfg.safe_mode and love and love.update and not safe_mode_updatehook and not safe_mode_bypass then
  local prev_update = love.update
  love.update = function(...)
    ensure_safe_mode_errorhandler()
    return prev_update(...)
  end
  safe_mode_updatehook = true
end
end

local ok, err = xpcall(bmm_init, function(e)
  if debug and debug.traceback then
    return debug.traceback(e)
  end
  return tostring(e)
end)
if not ok then
  bmm_init_log("init error: " .. tostring(err))
end
"#;

pub fn sync_compat_helper(enabled: bool) -> Result<(), String> {
    let mods_dir = local_mod_detection::resolve_mods_dir_path()?;
    ensure_helper_files(&mods_dir)?;
    write_config(enabled, &mods_dir)?;
    Ok(())
}

fn ensure_helper_files(mods_dir: &Path) -> Result<(), String> {
    let base_dir = mods_dir.join(MOD_FOLDER_NAME);
    let lovely_dir = base_dir.join("lovely");
    let helper_dir = base_dir.join("bmm_compat");

    fs::create_dir_all(&lovely_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&helper_dir).map_err(|e| e.to_string())?;

    let modules_path = lovely_dir.join("modules.toml");
    if modules_path.exists() {
        fs::remove_file(&modules_path).map_err(|e| e.to_string())?;
    }
    write_if_changed(helper_dir.join("init.lua"), INIT_LUA)?;
    write_if_changed(base_dir.join("lovely.toml"), LOVELY_TOML)?;
    write_if_changed(helper_dir.join("bootstrap.lua"), BOOTSTRAP_LUA)?;
    write_mods_index(mods_dir, &base_dir)?;
    Ok(())
}

fn write_mods_index(mods_dir: &Path, base_dir: &Path) -> Result<(), String> {
    let mut entries = Vec::new();
    let read_dir = fs::read_dir(mods_dir).map_err(|e| e.to_string())?;
    for entry in read_dir {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.is_empty() {
            continue;
        }
        entries.push(name);
    }
    entries.sort();
    let mut contents = String::new();
    for name in entries {
        contents.push_str(&name);
        contents.push('\n');
    }
    write_if_changed(base_dir.join(MODS_INDEX_FILE_NAME), &contents)?;
    Ok(())
}

fn write_config(enabled: bool, mods_dir: &Path) -> Result<(), String> {
    let mods_dir_str = mods_dir.to_string_lossy();
    let contents = if enabled {
        format!("enabled=true\nsafe_mode=true\nmods_dir={}\n", mods_dir_str)
    } else {
        format!("enabled=false\nsafe_mode=true\nmods_dir={}\n", mods_dir_str)
    };
    let mut targets: Vec<PathBuf> = Vec::new();
    if let Some(config_dir) = dirs::config_dir() {
        targets.push(config_dir.join("Balatro"));
    }
    if let Some(parent) = mods_dir.parent() {
        targets.push(parent.to_path_buf());
    }
    targets.sort();
    targets.dedup();

    let mut wrote_any = false;
    let mut last_err: Option<String> = None;
    for dir in targets {
        if let Err(e) = fs::create_dir_all(&dir).map_err(|e| e.to_string()) {
            last_err = Some(e);
            continue;
        }
        let config_path = dir.join(CONFIG_FILE_NAME);
        match write_if_changed(config_path, &contents) {
            Ok(_) => wrote_any = true,
            Err(e) => last_err = Some(e),
        }
    }
    if wrote_any {
        Ok(())
    } else {
        Err(last_err.unwrap_or_else(|| "Failed to write config".to_string()))
    }
}

fn write_if_changed(path: PathBuf, contents: &str) -> Result<(), String> {
    if let Ok(existing) = fs::read_to_string(&path)
        && existing == contents
    {
        return Ok(());
    }
    fs::write(&path, contents).map_err(|e| e.to_string())
}
