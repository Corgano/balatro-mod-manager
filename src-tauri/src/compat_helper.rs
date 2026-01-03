use std::fs;
use std::path::{Path, PathBuf};

use bmm_lib::local_mod_detection;

const MOD_FOLDER_NAME: &str = "BMM-Compat";
const CONFIG_FILE_NAME: &str = "bmm_compat.cfg";

const MODULES_TOML: &str = r#"[manifest]
version = "0.1.0"
dump_lua = true
priority = -100

[[patches]]
[patches.module]
source = "bmm_compat/init.lua"
before = "main.lua"
name = "bmm_compat.init"
"#;

const LOVELY_TOML: &str = r#"[manifest]
version = "0.1.0"
dump_lua = true
priority = -100

[[patches]]
[patches.copy]
target = "main.lua"
position = "append"
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

local ok, err = pcall(require, "bmm_compat.init")
if not ok then
  log_bootstrap("failed to require bmm_compat.init: " .. tostring(err))
end
"#;

const INIT_LUA: &str = r#"local function read_config()
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
    if love and love.filesystem and love.filesystem.append and path:match("^/") == nil then
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
  if love.filesystem.append and path:match("^/") == nil then
    pcall(love.filesystem.append, path, ("[%s] %s\n"):format(stamp, line))
    return
  end
  local ok, file = pcall(io.open, path, "a")
  if ok and file then
    file:write(("[%s] %s\n"):format(stamp, line))
    file:close()
  end
end

local function trim(s)
  return (s:gsub("^%s+", ""):gsub("%s+$", ""))
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

local function list_dir(path)
  if love and love.filesystem then
    if mounted_mods_dir ~= path then
      pcall(love.filesystem.unmount, "__bmm_mods")
      local ok_mount = pcall(love.filesystem.mount, path, "__bmm_mods")
      if ok_mount then
        mounted_mods_dir = path
      end
    end
    local ok_items, items = pcall(love.filesystem.getDirectoryItems, "__bmm_mods")
    if ok_items and type(items) == "table" then
      return items
    end
  end
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

local function list_lua_candidates(path)
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
  local ok, fh = pcall(io.open, path, "r")
  if ok and fh then
    fh:close()
    return true
  end
  return false
end

local function has_multiplayer_fallback(mods_dir)
  local base = mods_dir .. "/Multiplayer"
  local candidates = {
    base .. "/ui/smods.lua",
    base .. "/Multiplayer.lua",
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

local function parse_legacy_deps(mod_path)
  local deps = {}
  local mod_id = nil
  local files = list_lua_candidates(mod_path)
  for _, file in ipairs(files) do
    local ok, fh = pcall(io.open, file, "r")
    if ok and fh then
      local i = 0
      for line in fh:lines() do
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
          fh:close()
          return deps, mod_id
        end
        if i >= 120 then
          break
        end
      end
      fh:close()
    end
  end
  return deps, mod_id
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
  if SMODS and type(SMODS.version) == "string" then
    smods_version = SMODS.version
  end
  for _, name in ipairs(items) do
    if type(name) == "string" then
      local lower = name:lower()
      if lower:find("multiplayer") then
        has_multiplayer = true
      end
      if lower:find("smods") or lower:find("steamodded") then
        local v = name:match("smods%-([%w%.%-%_~]+)")
        if v then
          smods_version = smods_version or v
        else
          smods_unknown = true
        end
      end
    end
  end
  if not has_multiplayer and has_multiplayer_fallback(mods_dir) then
    has_multiplayer = true
    log_line("warn multiplayer_detected_fallback")
  end
  for _, name in ipairs(items) do
    if type(name) == "string" then
      local lower = name:lower()
      if lower:find("lovely") or lower == "bmm-compat" then
        -- skip
      else
        local mod_path = mods_dir .. "/" .. name
        local meta_path = mod_path .. "/smods.json"
        local okf, file = pcall(io.open, meta_path, "r")
        if okf and file then
          local data = file:read("*a") or ""
          file:close()
          local mod_id = data:match('"id"%s*:%s*"([^"]+)"') or name
          local deps_block = data:match('"dependencies"%s*:%s*%[(.-)%]')
          if deps_block then
            for dep in deps_block:gmatch('"([^"]+)"') do
              if type(dep) == "string" and dep:lower():find("steamodded") then
                local constraints = extract_constraints(dep)
                if smods_version then
                  local okv, unk = check_constraints(smods_version, constraints)
                  if okv == false then
                    issues[#issues + 1] = ("Steamodded version mismatch for %s: %s (have %s)"):format(
                      mod_id,
                      dep,
                      smods_version
                    )
                  elseif unk then
                    issues[#issues + 1] = ("Steamodded version check skipped for %s: %s (have %s)"):format(
                      mod_id,
                      dep,
                      smods_version
                    )
                  end
                else
                  issues[#issues + 1] = ("Steamodded version unknown for %s: %s"):format(
                    mod_id,
                    dep
                  )
                end
              end
            end
          end
        end
        if not (okf and file) then
          local deps, legacy_id = parse_legacy_deps(mod_path)
          local mod_id = legacy_id or name
          for _, dep in ipairs(deps) do
            if type(dep) == "string" and dep:lower():find("steamodded") then
              local constraints = extract_constraints(dep)
              if smods_version then
                local okv, unk = check_constraints(smods_version, constraints)
                if okv == false then
                  issues[#issues + 1] = ("Steamodded version mismatch for %s: %s (have %s)"):format(
                    mod_id,
                    dep,
                    smods_version
                  )
                elseif unk then
                  issues[#issues + 1] = ("Steamodded version check skipped for %s: %s (have %s)"):format(
                    mod_id,
                    dep,
                    smods_version
                  )
                end
              else
                issues[#issues + 1] = ("Steamodded version unknown for %s: %s"):format(
                  mod_id,
                  dep
                )
              end
            end
          end
        end
      end
    end
  end
  if has_multiplayer then
    if not smods_version or smods_unknown then
      issues[#issues + 1] = "Steamodded version unknown for Multiplayer (requires >=1.0.0)"
    else
      local cmp = compare_version(smods_version, "1.0.0")
      if cmp and cmp < 0 then
        issues[#issues + 1] = ("Steamodded %s is too old for Multiplayer (requires >=1.0.0)"):format(
          smods_version
        )
      end
    end
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

local function fail_with_issues(issues)
  if #issues == 0 then
    return
  end
  local msg = "BMM compatibility checks found issues:\n\n"
  local limit = math.min(8, #issues)
  for i = 1, limit do
    msg = msg .. "- " .. issues[i] .. "\n"
  end
  if #issues > limit then
    msg = msg .. string.format("...and %d more.\n", #issues - limit)
  end
  msg = msg .. "\nPlease update Steamodded or disable incompatible mods."
  log_line("fatal " .. msg:gsub("\n", " | "))
  if love and love.window and love.window.showMessageBox then
    pcall(love.window.showMessageBox, "BMM Compatibility", msg, "error")
    if love.event and love.event.quit then
      love.event.quit()
      return
    end
  end
  error(msg)
end

local function audit_smods()
  if not (SMODS and SMODS.Mods) then
    log_line("SMODS not detected at init.")
    return
  end
  log_line("SMODS detected at init.")

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

  if type(SMODS.register_mod) ~= "function" then
    log_line("warn api_missing path=SMODS.register_mod")
  end

  fail_with_issues(issues)
end

log_line("Compatibility helper enabled.")
local audit_done = false
local preflight_done = false

local function maybe_audit()
  flush_pending()
  if audit_done then
    return
  end
  if SMODS and SMODS.Mods then
    audit_done = true
    audit_smods()
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
    return prev_update(...)
  end
end

if not audit_done and debug and debug.sethook then
  debug.sethook(function()
    maybe_audit()
    if audit_done then
      debug.sethook()
    end
  end, "", 10000)
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

    write_if_changed(lovely_dir.join("modules.toml"), MODULES_TOML)?;
    write_if_changed(helper_dir.join("init.lua"), INIT_LUA)?;
    write_if_changed(base_dir.join("lovely.toml"), LOVELY_TOML)?;
    write_if_changed(helper_dir.join("bootstrap.lua"), BOOTSTRAP_LUA)?;
    Ok(())
}

fn write_config(enabled: bool, mods_dir: &Path) -> Result<(), String> {
    let config_dir =
        dirs::config_dir().ok_or_else(|| "Failed to resolve config directory".to_string())?;
    let balatro_dir = config_dir.join("Balatro");
    fs::create_dir_all(&balatro_dir).map_err(|e| e.to_string())?;
    let config_path = balatro_dir.join(CONFIG_FILE_NAME);
    let mods_dir = mods_dir.to_string_lossy();
    let contents = if enabled {
        format!("enabled=true\nmods_dir={}\n", mods_dir)
    } else {
        format!("enabled=false\nmods_dir={}\n", mods_dir)
    };
    write_if_changed(config_path, &contents)?;
    Ok(())
}

fn write_if_changed(path: PathBuf, contents: &str) -> Result<(), String> {
    if let Ok(existing) = fs::read_to_string(&path)
        && existing == contents
    {
        return Ok(());
    }
    fs::write(&path, contents).map_err(|e| e.to_string())
}
