-- Kaku Configuration

local wezterm = require 'wezterm'

local config = {}

-- `config_builder` validates every assignment and is expensive on large configs.
-- Keep startup fast by default; enable strict validation only when debugging config.
if os.getenv('KAKU_STRICT_CONFIG') == '1' and wezterm.config_builder then
  config = wezterm.config_builder()
end



local function basename(path)
  return path:match('([^/]+)$')
end

-- URL decode helper for Chinese characters in paths
-- Converts %E9%9F%B3%E4%B9%90 -> 音乐
local function url_decode(str)
  if not str then
    return str
  end
  -- First, handle UTF-8 encoded sequences (%XX%YY%ZZ)
  local result = str:gsub('%%([0-9A-Fa-f][0-9A-Fa-f])', function(hex)
    return string.char(tonumber(hex, 16))
  end)
  return result
end

local function padding_matches(current, expected)
  return current
    and current.left == expected.left
    and current.right == expected.right
    and current.top == expected.top
    and current.bottom == expected.bottom
end

local fullscreen_uniform_padding = {
  left = '40px',
  right = '40px',
  top = '70px',
  bottom = '30px',
}

-- Per-window resize debounce state.
-- Weak keys ensure closed windows don't leak state.
local resize_state_by_window = setmetatable({}, { __mode = 'k' })

local function monotonic_now()
  -- Keep this numeric for debounce arithmetic.
  -- Some runtime environments expose os.clock() as a non-number value.
  local ok, value = pcall(os.clock)
  if ok and type(value) == 'number' then
    return value
  end
  return os.time()
end

local function dims_hash(dims)
  return dims.pixel_width .. "x" .. dims.pixel_height
end

local function update_window_config(window, is_full_screen)
  local now = monotonic_now()
  local dims = window:get_dimensions()
  local current_hash = dims_hash(dims)
  local state = resize_state_by_window[window]
  if not state then
    state = {
      last_resize_time = 0,
      last_dims_hash = "",
    }
    resize_state_by_window[window] = state
  end
  local overrides = window:get_config_overrides() or {}
  local needs_update = false

  if is_full_screen then
    needs_update = (not padding_matches(overrides.window_padding, fullscreen_uniform_padding))
      or overrides.hide_tab_bar_if_only_one_tab ~= false
  else
    needs_update = overrides.window_padding ~= nil or overrides.hide_tab_bar_if_only_one_tab ~= nil
  end

  -- Skip update if dimensions changed rapidly (within 1 second) and state is stable
  -- This prevents padding flicker during fullscreen animation
  if current_hash ~= state.last_dims_hash then
    local time_since_last = now - state.last_resize_time
    if time_since_last < 1.0 and not needs_update then
      -- Rapid change detected, skip this update
      state.last_dims_hash = current_hash
      state.last_resize_time = now
      return
    end
    state.last_dims_hash = current_hash
    state.last_resize_time = now
  end

  if is_full_screen then
    if not padding_matches(overrides.window_padding, fullscreen_uniform_padding) or overrides.hide_tab_bar_if_only_one_tab ~= false then
      overrides.window_padding = fullscreen_uniform_padding
      overrides.hide_tab_bar_if_only_one_tab = false
      window:set_config_overrides(overrides)
    end
    return
  end

  if overrides.window_padding ~= nil or overrides.hide_tab_bar_if_only_one_tab ~= nil then
    overrides.window_padding = nil
    overrides.hide_tab_bar_if_only_one_tab = nil
    window:set_config_overrides(overrides)
  end
end

local function extract_path_from_cwd(cwd)
  if not cwd then
    return ''
  end

  local path = ''
  if type(cwd) == 'table' then
    path = cwd.file_path or cwd.path or tostring(cwd)
  else
    path = tostring(cwd)
  end

  path = path:gsub('^file://[^/]*', ''):gsub('/$', '')
  -- Decode URL-encoded characters (e.g., %E9%9F%B3%E4%B9%90 -> 音乐)
  path = url_decode(path)
  return path
end

local active_tab_cwd_cache = {}
-- os.time() returns integer wall-clock seconds; 1s granularity is fine for tab title throttle
local active_tab_cwd_refresh_interval = 1
local function now_secs()
  return os.time()
end

local home_dir = os.getenv("HOME")
local kaku_state_dir = home_dir and (home_dir .. "/.config/kaku") or nil
local lazygit_state_file = kaku_state_dir and (kaku_state_dir .. "/lazygit_state.json") or nil
local lazygit_state_cache = nil
local lazygit_repo_probe_cache = {}
local lazygit_repo_probe_interval_secs = 5
local lazygit_command_probe = { value = nil, command = nil, checked_at = 0 }
local lazygit_command_probe_interval_secs = 30

local function trim_trailing_whitespace(value)
  if type(value) ~= "string" then
    return ""
  end
  return value:gsub("%s+$", "")
end

local function trim_surrounding_whitespace(value)
  if type(value) ~= "string" then
    return ""
  end
  return value:gsub("^%s+", ""):gsub("%s+$", "")
end

local function normalize_ai_summary(value, fallback)
  local text = trim_surrounding_whitespace(value or "")
  if text == "" then
    text = fallback or "Fix the command and retry."
  end

  text = text:gsub("[\r\n]+", " ")
  text = text:gsub("[\"']", "")
  text = text:gsub("%b()", "")
  text = text:gsub("^%s*[Tt]he%s+[Uu]ser%s+typed%s+", "")
  text = text:gsub("^%s*[Yy]ou%s+typed%s+", "")
  text = text:gsub("^%s*[Tt]his%s+command%s+", "Command ")
  text = text:gsub("^%s*[Cc]ommand%s+[Ff]ailed%s*:?%s*", "")
  text = text:gsub("^%s*[Ff]ailed%s*:?%s*", "")
  text = text:gsub("^%s*[Ee]rror%s*:?%s*", "")
  text = text:gsub("^%s*[Tt]he%s+command%s+", "Command ")
  text = text:gsub("%s+which%s+is%s+not%s+a%s+valid%s+", " is not a valid ")
  text = text:gsub("%s+is%s+not%s+recognized%s+", " is not recognized ")
  text = text:gsub("%s+maybe%s+the%s+user%s+meant.*$", "")
  text = text:gsub("%s+maybe%s+you%s+meant.*$", "")
  text = text:gsub("%s+maybe%s+.*$", "")
  text = text:gsub("%s+git%s+suggests.*$", "")
  text = text:gsub("%s+did%s+you%s+mean.*$", "")
  text = text:gsub("%s+", " ")
  text = trim_surrounding_whitespace(text)

  local sentence_end = text:find("[%.%!%?。！？]")
  if sentence_end and sentence_end > 0 then
    text = text:sub(1, sentence_end)
  end

  local max_chars = 72
  if #text > max_chars then
    local shortened = text:sub(1, max_chars)
    shortened = shortened:gsub("%s+[^%s]*$", "")
    if shortened == "" then
      shortened = text:sub(1, max_chars)
    end
    text = trim_surrounding_whitespace(shortened)
  end

  if text == "" then
    return "Fix the command and retry."
  end
  if not text:find("[%.%!%?。！？]$") then
    text = text .. "."
  end
  return text
end

local function strip_wrapping_quotes(value)
  if type(value) ~= "string" then
    return ""
  end
  local trimmed = trim_surrounding_whitespace(value)
  local first = trimmed:sub(1, 1)
  local last = trimmed:sub(-1)
  if #trimmed >= 2 and ((first == "'" and last == "'") or (first == '"' and last == '"')) then
    return trimmed:sub(2, -2)
  end
  return trimmed
end

local ai_fix_toml_path = kaku_state_dir and (kaku_state_dir .. "/assistant.toml") or nil
local ai_fix_file_settings = {}

local function strip_inline_toml_comment(line)
  if type(line) ~= "string" then
    return ""
  end

  local in_single = false
  local in_double = false
  local escaped = false
  local index = 1

  while index <= #line do
    local ch = line:sub(index, index)
    if in_double then
      if escaped then
        escaped = false
      elseif ch == "\\" then
        escaped = true
      elseif ch == '"' then
        in_double = false
      end
    elseif in_single then
      if ch == "'" then
        in_single = false
      end
    else
      if ch == '"' then
        in_double = true
      elseif ch == "'" then
        in_single = true
      elseif ch == "#" then
        return trim_surrounding_whitespace(line:sub(1, index - 1))
      end
    end
    index = index + 1
  end

  return trim_surrounding_whitespace(line)
end

local function parse_ai_toml_setting_value(raw_value)
  local value = trim_surrounding_whitespace(raw_value or "")
  if value == "" then
    return nil
  end

  if value:sub(1, 1) == "[" and value:sub(-1) == "]" then
    local items = {}
    local content = trim_surrounding_whitespace(value:sub(2, -2))
    if content ~= "" then
      for part in content:gmatch("[^,]+") do
        local item = strip_wrapping_quotes(part)
        if item ~= "" then
          items[#items + 1] = item
        end
      end
    end
    return table.concat(items, ",")
  end

  if value == "true" then
    return true
  end
  if value == "false" then
    return false
  end

  local number_value = tonumber(value)
  if number_value ~= nil then
    return number_value
  end

  return strip_wrapping_quotes(value)
end

local function load_ai_fix_file_settings()
  local settings = {}
  if not ai_fix_toml_path or ai_fix_toml_path == "" then
    return settings
  end

  local file = io.open(ai_fix_toml_path, "r")
  if not file then
    return settings
  end

  for raw_line in file:lines() do
    local line = strip_inline_toml_comment(raw_line)
    if line ~= "" then
      if not line:match("^%s*%[") then
        local key, raw_value = line:match("^%s*([%w_%-]+)%s*=%s*(.-)%s*$")
        if key and raw_value then
          local parsed = parse_ai_toml_setting_value(raw_value)
          if parsed ~= nil then
            settings[key] = parsed
          end
        end
      end
    end
  end

  file:close()
  return settings
end

local function stringify_ai_setting_value(value)
  if value == nil then
    return nil
  end
  if type(value) == "boolean" then
    return value and "1" or "0"
  end
  if type(value) == "number" then
    return tostring(value)
  end
  if type(value) == "string" then
    local normalized = trim_surrounding_whitespace(value)
    if normalized == "" then
      return nil
    end
    if normalized == "true" then
      return "1"
    end
    if normalized == "false" then
      return "0"
    end
    return normalized
  end
  return nil
end

local function read_ai_setting(file_key, default_value)
  local value = stringify_ai_setting_value(ai_fix_file_settings[file_key])
  if not value or value == "" then
    return default_value
  end
  return value
end

ai_fix_file_settings = load_ai_fix_file_settings()

local ai_fix_enabled = read_ai_setting("enabled", "1") ~= "0"
local ai_fix_api_base_url = read_ai_setting("base_url", "https://api.vivgrid.com/v1")
local ai_fix_api_key = read_ai_setting("api_key", nil)
local ai_fix_model = read_ai_setting("model", "gpt-5-mini")
local ai_fix_timeout_secs = 12
local ai_fix_debug_enabled = false
local ai_fix_state_by_pane = {}
local ai_fix_poll_interval_secs = 0.2
local ai_fix_poll_deadline_secs = ai_fix_timeout_secs + 4
local ai_fix_jobs_dir = kaku_state_dir and (kaku_state_dir .. "/ai_jobs") or "/tmp"
local ai_fix_job_counter = 0

local function ai_debug_log(message)
  if not ai_fix_debug_enabled then
    return
  end
  local now = os.date("!%Y-%m-%dT%H:%M:%SZ")
  local file = io.open("/tmp/kaku_ai_debug.log", "a")
  if not file then
    return
  end
  file:write(now .. " " .. tostring(message) .. "\n")
  file:close()
end

local function refresh_ai_fix_settings()
  ai_fix_file_settings = load_ai_fix_file_settings()
  ai_fix_enabled = read_ai_setting("enabled", ai_fix_enabled and "1" or "0") ~= "0"
  ai_fix_api_base_url = read_ai_setting("base_url", ai_fix_api_base_url)
  ai_fix_api_key = read_ai_setting("api_key", ai_fix_api_key)
  ai_fix_model = read_ai_setting("model", ai_fix_model)
end

local function detect_git_branch(path)
  if not path or path == "" then
    return ""
  end

  local ok, stdout = wezterm.run_child_process({
    "git",
    "-C",
    path,
    "rev-parse",
    "--abbrev-ref",
    "HEAD",
  })
  if not ok then
    return ""
  end
  return trim_trailing_whitespace(stdout)
end

local function build_ai_fix_messages(failed_command, exit_code, cwd, git_branch)
  local context = {
    "Command: " .. failed_command,
    "Exit code: " .. tostring(exit_code),
    "Working directory: " .. (cwd ~= "" and cwd or "(unknown)"),
  }

  if git_branch ~= "" then
    context[#context + 1] = "Git branch: " .. git_branch
  end

  return {
    {
      role = "system",
      content = "You are a shell troubleshooting assistant. Output English only and return exactly one JSON object with keys summary, command, why, confidence. Do not use markdown or code fences. summary must be one concise sentence <= 72 chars and must not contain parentheses. command must be a single direct fix command without commentary. Avoid diagnostic-only commands, alias probing, and placeholders. Never use aliases like ll. If you are not confident about a direct fix, set command to an empty string.",
    },
    {
      role = "user",
      content = table.concat(context, "\n"),
    },
  }
end

local function ai_fix_endpoint()
  return trim_trailing_whitespace(ai_fix_api_base_url):gsub("/+$", "") .. "/chat/completions"
end

local function encode_ai_fix_payload(model, messages)
  local payload_ok, payload = pcall(wezterm.json_encode, {
    model = model,
    messages = messages,
    stream = false,
  })
  if not payload_ok or type(payload) ~= "string" or payload == "" then
    return nil, "failed to encode request payload"
  end
  return payload
end

local function next_ai_fix_job_id()
  ai_fix_job_counter = ai_fix_job_counter + 1
  return tostring(now_secs()) .. "-" .. tostring(ai_fix_job_counter)
end

local function ensure_ai_fix_jobs_dir()
  if not ai_fix_jobs_dir or ai_fix_jobs_dir == "" then
    return false
  end
  os.execute(string.format("mkdir -p %q", ai_fix_jobs_dir))
  return true
end

local function write_text_file(path, content)
  local file = io.open(path, "w")
  if not file then
    return false
  end
  file:write(content or "")
  file:close()
  return true
end

local function read_text_file(path)
  local file = io.open(path, "r")
  if not file then
    return nil
  end
  local content = file:read("*a")
  file:close()
  return content
end

local function cleanup_ai_fix_job_files(job)
  if not job or not job.paths then
    return
  end
  os.remove(job.paths.request_path)
  os.remove(job.paths.response_path)
  os.remove(job.paths.stderr_path)
  os.remove(job.paths.status_path)
end

local function start_ai_fix_background_job(payload)
  if not ensure_ai_fix_jobs_dir() then
    return nil, "state directory unavailable"
  end

  local job_id = next_ai_fix_job_id()
  local base_path = ai_fix_jobs_dir .. "/ai_fix_" .. job_id
  local job = {
    id = job_id,
    started_at = now_secs(),
    paths = {
      request_path = base_path .. ".request.json",
      response_path = base_path .. ".response.json",
      stderr_path = base_path .. ".stderr.log",
      status_path = base_path .. ".status",
    },
  }

  if not write_text_file(job.paths.request_path, payload) then
    return nil, "failed to write request payload"
  end

  local script = [[
status=0
curl -sS --fail --connect-timeout "$1" --max-time "$2" "$3" \
  -H "$4" \
  -H "$5" \
  --data-binary "@$6" \
  -o "$7" \
  --stderr "$8"
status=$?
printf '%s' "$status" > "$9"
]]
  local launched_ok, launch_err = pcall(function()
    wezterm.background_child_process({
      "sh",
      "-c",
      script,
      "kaku-ai-fix",
      "3",
      tostring(ai_fix_timeout_secs),
      ai_fix_endpoint(),
      "Authorization: Bearer " .. ai_fix_api_key,
      "Content-Type: application/json",
      job.paths.request_path,
      job.paths.response_path,
      job.paths.stderr_path,
      job.paths.status_path,
    })
  end)

  if not launched_ok then
    cleanup_ai_fix_job_files(job)
    return nil, trim_surrounding_whitespace(tostring(launch_err or "failed to launch request"))
  end

  return job
end

local function extract_assistant_message_content(response)
  if type(response) ~= "table" then
    return nil
  end
  local choices = response.choices
  if type(choices) ~= "table" or #choices == 0 then
    return nil
  end
  local message = choices[1] and choices[1].message
  if type(message) ~= "table" then
    return nil
  end

  local content = message.content
  if type(content) == "string" then
    return content
  end
  if type(content) ~= "table" then
    return nil
  end

  local chunks = {}
  for _, item in ipairs(content) do
    if type(item) == "table" and type(item.text) == "string" then
      chunks[#chunks + 1] = item.text
    elseif type(item) == "string" then
      chunks[#chunks + 1] = item
    end
  end
  if #chunks == 0 then
    return nil
  end
  return table.concat(chunks, "\n")
end

local function parse_json_object_from_text(value)
  if type(value) ~= "string" then
    return nil
  end

  local text = trim_surrounding_whitespace(value)
  local direct_ok, parsed = pcall(wezterm.json_parse, text)
  if direct_ok and type(parsed) == "table" then
    return parsed
  end

  local first_brace = text:find("{", 1, true)
  local last_brace = nil
  for idx = #text, 1, -1 do
    if text:sub(idx, idx) == "}" then
      last_brace = idx
      break
    end
  end
  if not first_brace or not last_brace or last_brace < first_brace then
    return nil
  end

  local candidate = text:sub(first_brace, last_brace)
  local nested_ok, nested = pcall(wezterm.json_parse, candidate)
  if nested_ok and type(nested) == "table" then
    return nested
  end
  return nil
end

local function sanitize_suggested_command(value)
  if type(value) ~= "string" then
    return ""
  end

  local command = trim_surrounding_whitespace(value)
  command = command:gsub("\r", "")
  command = command:gsub("^```[%w_-]*\n?", "")
  command = command:gsub("\n```$", "")
  command = trim_surrounding_whitespace(command)

  local first_line = command:match("([^\n]+)") or command
  first_line = trim_surrounding_whitespace(first_line)
  first_line = first_line:gsub("^%$%s*", "")
  return first_line
end

local function is_dangerous_command(command)
  if type(command) ~= "string" or command == "" then
    return false
  end
  local lower = trim_surrounding_whitespace(command:lower())
  if lower == "" then
    return false
  end

  if lower:match("^:%(%){:%|:&};:$") then
    return true
  end

  if lower:match("^%s*sudo%s+.*%srm%s+.-%-%w*r%w*f%w*") or lower:match("^%s*sudo%s+.*%srm%s+.-%-%w*f%w*r%w*") then
    return true
  end

  local normalized = lower:gsub("^%s*sudo%s+", "")
  if normalized:match("^%s*rm%s+.-%-%w*r%w*f%w*") or normalized:match("^%s*rm%s+.-%-%w*f%w*r%w*") then
    return true
  end

  local patterns = {
    "^%s*mkfs",
    "^%s*dd%s+if=",
    "^%s*shutdown",
    "^%s*reboot",
    "^%s*poweroff",
    "^%s*git%s+reset%s+%-%-hard",
    "^%s*git%s+clean%s+%-[%w]*f[%w]*d",
  }
  for _, pattern in ipairs(patterns) do
    if normalized:match(pattern) then
      return true
    end
  end
  return false
end

local function is_non_actionable_ai_command(command)
  local normalized = trim_surrounding_whitespace(command or "")
  if normalized == "" then
    return false
  end
  local lower = normalized:lower()
  local patterns = {
    "^%s*type%s+",
    "^%s*which%s+",
    "^%s*command%s+%-v%s+",
    "^%s*ll%s*$",
    "^%s*ll%s+",
    "%|%|",
    "&&%s*echo",
    ";%s*echo",
  }
  for _, pattern in ipairs(patterns) do
    if lower:match(pattern) then
      return true
    end
  end
  return false
end

local function parse_ai_fix_result(content)
  local parsed = parse_json_object_from_text(content or "")
  if type(parsed) == "table" then
    local summary = normalize_ai_summary(tostring(parsed.summary or ""), "Fix the command and retry.")
    local command = sanitize_suggested_command(parsed.command or "")
    local why = trim_surrounding_whitespace(tostring(parsed.why or ""))
    local confidence = tonumber(parsed.confidence) or 0
    return {
      summary = summary,
      command = command,
      why = why,
      confidence = confidence,
    }
  end

  local first_line = normalize_ai_summary((content or ""):match("([^\n]+)") or "", "Fix the command and retry.")
  return {
    summary = first_line,
    command = "",
    why = "",
    confidence = 0,
  }
end

local function parse_ai_fix_response(stdout)
  local parsed_ok, response = pcall(wezterm.json_parse, stdout or "")
  if not parsed_ok or type(response) ~= "table" then
    return nil, "invalid json response"
  end
  local content = extract_assistant_message_content(response)
  if not content or content == "" then
    return nil, "empty model response"
  end

  local result = parse_ai_fix_result(content)
  if result.command ~= "" and is_non_actionable_ai_command(result.command) then
    result.command = ""
  end
  return result
end

local function inject_ai_notice(pane, headline, detail, suggested_command)
  if not pane then
    return
  end

  local summary = trim_surrounding_whitespace(headline or "Update")
  local normalized_detail = trim_surrounding_whitespace(detail or "")
  if normalized_detail ~= "" and summary ~= "" then
    summary = summary .. ": " .. normalized_detail
  elseif normalized_detail ~= "" then
    summary = normalized_detail
  end

  local cmd = sanitize_suggested_command(suggested_command or "")
  local summary_line = "\27[38;5;141m╭─ Kaku Assistant\27[0m  \27[1m" .. summary .. "\27[0m"
  local command_line = ""
  if cmd ~= "" then
    command_line = "\27[38;5;141m╰─\27[0m " .. cmd .. "    \27[38;5;244mCmd+Shift+E\27[0m"
  else
    command_line = "\27[38;5;141m╰─\27[0m \27[38;5;244mNo safe command suggested\27[0m"
  end

  local output = "\r\n" .. summary_line .. "\r\n" .. command_line
  pcall(function()
    pane:inject_output(output)
  end)
end

local function finalize_shell_line(pane)
  if not pane then
    return
  end
  -- Ensure prompt returns to an input-ready state without manual Enter.
  pcall(function()
    pane:send_text("\n")
  end)
end

local function inject_ai_status(pane, message)
  if not pane then
    return
  end

  local summary = normalize_ai_summary(message or "", "Checking this error now.")
  local line = "\27[38;5;141m╰─ Kaku Assistant\27[0m  \27[38;5;244m" .. summary .. "\27[0m"
  local output = "\r\n" .. line .. "\r\n"
  pcall(function()
    pane:inject_output(output)
  end)
end

local function show_ai_loading_toast(window, pane)
  if not window or not pane then
    return
  end
  pcall(function()
    window:perform_action(wezterm.action.EmitEvent("kaku-toast-ai-analyzing"), pane)
  end)
end

local function clear_ai_fix_suggestion_state(pane_state)
  if not pane_state then
    return
  end
  pane_state.suggestion = nil
  pane_state.generated_at = nil
end

local function poll_ai_fix_job(window, pane, pane_id, job, failed_command, exit_code)
  local pane_state = ai_fix_state_by_pane[pane_id]
  if not pane_state or pane_state.pending_job_id ~= job.id then
    cleanup_ai_fix_job_files(job)
    return
  end

  local status_text = read_text_file(job.paths.status_path)
  local status_value = trim_surrounding_whitespace(status_text or "")
  if status_value == "" then
    if (now_secs() - job.started_at) >= ai_fix_poll_deadline_secs then
      pane_state.inflight = false
      pane_state.pending_job_id = nil
      clear_ai_fix_suggestion_state(pane_state)
      cleanup_ai_fix_job_files(job)
      ai_debug_log("ai_fix_job timeout pane_id=" .. pane_id)
      inject_ai_status(pane, "Could not analyze this error right now.")
      return
    end

    wezterm.time.call_after(ai_fix_poll_interval_secs, function()
      poll_ai_fix_job(window, pane, pane_id, job, failed_command, exit_code)
    end)
    return
  end

  local status_code = tonumber(status_value)
  local stdout = read_text_file(job.paths.response_path) or ""
  local stderr = read_text_file(job.paths.stderr_path) or ""
  cleanup_ai_fix_job_files(job)

  pane_state.inflight = false
  pane_state.pending_job_id = nil

  if status_code ~= 0 then
    clear_ai_fix_suggestion_state(pane_state)
    ai_debug_log("ai_fix_job failed pane_id=" .. pane_id .. " status=" .. tostring(status_code) .. " err=" .. tostring(stderr))
    inject_ai_status(pane, "Could not analyze this error right now.")
    return
  end

  local result, parse_err = parse_ai_fix_response(stdout)
  if not result then
    clear_ai_fix_suggestion_state(pane_state)
    ai_debug_log("ai_fix_job invalid_response pane_id=" .. pane_id .. " err=" .. tostring(parse_err))
    inject_ai_status(pane, "Could not analyze this error right now.")
    return
  end

  result.model = ai_fix_model
  pane_state.suggestion = result
  pane_state.failed_command = failed_command
  pane_state.exit_code = exit_code
  pane_state.generated_at = now_secs()

  local command = sanitize_suggested_command(result.command or "")
  if command == "" then
    local summary = normalize_ai_summary(result.summary or "", "No quick fix command found.")
    inject_ai_status(pane, summary)
    return
  end

  inject_ai_notice(
    pane,
    normalize_ai_summary(result.summary or "", "Fix the command and retry."),
    "",
    command
  )
  finalize_shell_line(pane)
end

local function request_ai_fix_async(window, pane, pane_id, failed_command, exit_code, cwd, git_branch)
  local messages = build_ai_fix_messages(failed_command, exit_code, cwd, git_branch)
  local payload, payload_err = encode_ai_fix_payload(ai_fix_model, messages)
  if not payload then
    return nil, payload_err or "failed to encode request payload"
  end

  local job, job_err = start_ai_fix_background_job(payload)
  if not job then
    return nil, job_err or "failed to start background request"
  end

  local pane_state = ai_fix_state_by_pane[pane_id]
  if pane_state then
    pane_state.pending_job_id = job.id
  end
  ai_debug_log("ai_fix_job started pane_id=" .. pane_id .. " job_id=" .. job.id)

  wezterm.time.call_after(ai_fix_poll_interval_secs, function()
    poll_ai_fix_job(window, pane, pane_id, job, failed_command, exit_code)
  end)
  return true
end

local function ensure_kaku_state_dir()
  if not kaku_state_dir or kaku_state_dir == "" then
    return
  end
  os.execute(string.format("mkdir -p %q", kaku_state_dir))
end

local function load_lazygit_state()
  if lazygit_state_cache then
    return lazygit_state_cache
  end

  local state = { repos = {} }
  if not lazygit_state_file then
    lazygit_state_cache = state
    return state
  end

  local file = io.open(lazygit_state_file, "r")
  if file then
    local raw = file:read("*all")
    file:close()
    if raw and raw ~= "" then
      local ok, parsed = pcall(wezterm.json_parse, raw)
      if ok and type(parsed) == "table" then
        state = parsed
      end
    end
  end

  if type(state.repos) ~= "table" then
    state.repos = {}
  end

  lazygit_state_cache = state
  return state
end

local function save_lazygit_state()
  if not lazygit_state_file then
    return
  end

  ensure_kaku_state_dir()
  local state = load_lazygit_state()
  local ok, encoded = pcall(wezterm.json_encode, state)
  if not ok or type(encoded) ~= "string" or encoded == "" then
    return
  end

  local file = io.open(lazygit_state_file, "w")
  if not file then
    return
  end
  file:write(encoded .. "\n")
  file:close()
end

local function get_lazygit_repo_flags(repo_root)
  local repos = load_lazygit_state().repos
  local flags = repos[repo_root]
  if type(flags) ~= "table" then
    flags = { hinted = false, used = false }
    repos[repo_root] = flags
  end
  flags.hinted = flags.hinted == true
  flags.used = flags.used == true
  return flags
end

local function mark_repo_lazygit_hinted(repo_root)
  if not repo_root or repo_root == "" then
    return
  end
  local flags = get_lazygit_repo_flags(repo_root)
  if flags.hinted then
    return
  end
  flags.hinted = true
  save_lazygit_state()
end

local function mark_repo_lazygit_used(repo_root)
  if not repo_root or repo_root == "" then
    return
  end
  local flags = get_lazygit_repo_flags(repo_root)
  if flags.used then
    return
  end
  flags.used = true
  save_lazygit_state()
end

local function pane_cwd(pane)
  if not pane then
    return ""
  end

  local ok, runtime_cwd = pcall(function()
    return pane:get_current_working_dir()
  end)
  if ok and runtime_cwd then
    local path = extract_path_from_cwd(runtime_cwd)
    if path ~= "" then
      return path
    end
  end

  return extract_path_from_cwd(pane.current_working_dir)
end

local function detect_git_repo_root(path)
  if not path or path == "" then
    return nil
  end

  local ok, stdout = wezterm.run_child_process({
    "git",
    "-C",
    path,
    "rev-parse",
    "--show-toplevel",
  })
  if not ok then
    return nil
  end

  local repo_root = trim_trailing_whitespace(stdout)
  if repo_root == "" then
    return nil
  end
  return repo_root
end

local function repo_has_pending_changes(repo_root)
  local ok, stdout = wezterm.run_child_process({
    "git",
    "-C",
    repo_root,
    "status",
    "--porcelain",
    "--untracked-files=normal",
  })
  if not ok then
    return false
  end
  return trim_trailing_whitespace(stdout) ~= ""
end

local function git_repo_context(path)
  local now = now_secs()
  local cached = lazygit_repo_probe_cache[path]
  if cached and (now - cached.checked_at) < lazygit_repo_probe_interval_secs then
    return cached.repo_root, cached.has_changes
  end

  local repo_root = detect_git_repo_root(path)
  local has_changes = false
  if repo_root then
    has_changes = repo_has_pending_changes(repo_root)
  end

  lazygit_repo_probe_cache[path] = {
    checked_at = now,
    repo_root = repo_root,
    has_changes = has_changes,
  }

  return repo_root, has_changes
end

local function resolve_lazygit_command()
  local now = now_secs()
  local cached_value = lazygit_command_probe.value
  if cached_value ~= nil then
    local age = now - lazygit_command_probe.checked_at
    if cached_value or age < lazygit_command_probe_interval_secs then
      return lazygit_command_probe.command
    end
  end

  local candidates = {
    "lazygit",
    "/opt/homebrew/bin/lazygit",
    "/usr/local/bin/lazygit",
  }
  local resolved = nil
  for _, cmd in ipairs(candidates) do
    local call_ok, run_result = pcall(function()
      return select(1, wezterm.run_child_process({ cmd, "--version" }))
    end)
    if call_ok and run_result then
      resolved = cmd
      break
    end
  end

  lazygit_command_probe.value = resolved ~= nil
  lazygit_command_probe.command = resolved
  lazygit_command_probe.checked_at = now
  return resolved
end

local function is_lazygit_installed()
  return resolve_lazygit_command() ~= nil
end

local function pane_is_lazygit(pane)
  if not pane then
    return false
  end

  local ok, proc = pcall(function()
    return pane:get_foreground_process_name()
  end)
  if not ok or type(proc) ~= "string" or proc == "" then
    return false
  end

  return basename(proc) == "lazygit"
end

local function resolve_active_pane(window, pane)
  if pane then
    return pane
  end
  if not window then
    return nil
  end

  local ok_tab, tab = pcall(function()
    return window:active_tab()
  end)
  if ok_tab and tab then
    local ok_pane, active_pane = pcall(function()
      return tab:active_pane()
    end)
    if ok_pane then
      return active_pane
    end
  end

  return nil
end

local function show_lazygit_toast(window, pane, event_name)
  if not window then
    return
  end
  pcall(function()
    window:perform_action(wezterm.action.EmitEvent(event_name), pane)
  end)
end

local function maybe_show_lazygit_hint(window, pane)
  pane = resolve_active_pane(window, pane)

  local path = pane_cwd(pane)
  if path == "" then
    return
  end

  local repo_root, has_changes = git_repo_context(path)
  if not repo_root then
    return
  end

  if pane_is_lazygit(pane) then
    mark_repo_lazygit_used(repo_root)
    return
  end

  local flags = get_lazygit_repo_flags(repo_root)
  if flags.hinted or flags.used or not has_changes then
    return
  end

  if not is_lazygit_installed() then
    return
  end

  show_lazygit_toast(window, pane, "kaku-toast-try-lazygit")
  mark_repo_lazygit_hinted(repo_root)
end

local function launch_lazygit(window, pane)
  pane = resolve_active_pane(window, pane)
  if not pane then
    show_lazygit_toast(window, pane, "kaku-toast-lazygit-no-pane")
    return
  end

  local path = pane_cwd(pane)
  if path == "" then
    show_lazygit_toast(window, pane, "kaku-toast-lazygit-no-cwd")
    return
  end

  local repo_root = detect_git_repo_root(path)
  if not repo_root then
    show_lazygit_toast(window, pane, "kaku-toast-lazygit-not-git")
    return
  end

  local lazygit_cmd = resolve_lazygit_command()
  if not lazygit_cmd then
    show_lazygit_toast(window, pane, "kaku-toast-lazygit-missing")
    return
  end

  local ok = pcall(function()
    -- Send Ctrl+U first to clear any partially typed input at the prompt,
    -- preventing the command from being appended to existing line content.
    window:perform_action(
      wezterm.action.SendString("\x15" .. lazygit_cmd .. "\r"),
      pane
    )
  end)
  if not ok then
    show_lazygit_toast(window, pane, "kaku-toast-lazygit-dispatch-failed")
    return
  end
  mark_repo_lazygit_used(repo_root)
end

local function evict_stale_cache(live_pane_ids)
  for pane_id in pairs(active_tab_cwd_cache) do
    if not live_pane_ids[pane_id] then
      active_tab_cwd_cache[pane_id] = nil
    end
  end
end

local function tab_path_parts(tab)
  local pane = tab.active_pane
  if not pane then
    return '', ''
  end

  local source_cwd = pane.current_working_dir
  local source_path = extract_path_from_cwd(source_cwd)
  local path = source_path

  if tab.is_active then
    local pane_id = tostring(pane.pane_id)
    local now = now_secs()
    local cached = active_tab_cwd_cache[pane_id]
    local should_refresh = (not cached)
      or path == ''
      or source_path ~= cached.source_path
      or (now - cached.updated_at) >= active_tab_cwd_refresh_interval

    if should_refresh then
      local ok, runtime_cwd = pcall(function()
        return pane:get_current_working_dir()
      end)
      if ok and runtime_cwd then
        local runtime_path = extract_path_from_cwd(runtime_cwd)
        if runtime_path ~= '' then
          path = runtime_path
        end
      end

      active_tab_cwd_cache[pane_id] = {
        path = path,
        source_path = source_path,
        updated_at = now,
      }
    elseif cached and cached.path ~= '' then
      path = cached.path
    end
  elseif path == '' then
    local ok, runtime_cwd = pcall(function()
      return pane:get_current_working_dir()
    end)
    if ok and runtime_cwd then
      path = extract_path_from_cwd(runtime_cwd)
    end
  end

  if path == '' then
    return '', ''
  end

  local current = basename(path) or path
  local parent_path = path:match('(.+)/[^/]+$') or ''
  local parent = basename(parent_path) or parent_path
  return parent, current
end

wezterm.on('format-tab-title', function(tab, tabs, _, effective_config, hover, max_width)
  -- Evict stale cache only on the first tab to avoid O(n²) across the render cycle
  if tab.tab_index == 0 then
    local live_pane_ids = {}
    for _, t in ipairs(tabs) do
      if t.active_pane then
        live_pane_ids[tostring(t.active_pane.pane_id)] = true
      end
    end
    evict_stale_cache(live_pane_ids)
  end

  local parent, current = tab_path_parts(tab)
  local text = current
  if parent ~= '' and current ~= '' then
    text = parent .. '/' .. current
  end

  -- Guard active_pane nil before accessing .title / .is_zoomed
  local active_pane = tab.active_pane
  if text == '' and active_pane then
    text = active_pane.title
  end
  if active_pane and active_pane.is_zoomed then
    text = text .. ' [Z]'
  end
  text = wezterm.truncate_right(text, math.max(8, max_width - 2))

  local intensity = tab.is_active and 'Bold' or 'Normal'
  -- resolved_palette.tab_bar and its sub-fields are all optional; guard each level
  local tab_bar_colors = effective_config.resolved_palette.tab_bar
  local fg
  if tab_bar_colors then
    local entry
    if tab.is_active then
      entry = tab_bar_colors.active_tab
    elseif hover then
      entry = tab_bar_colors.inactive_tab_hover or tab_bar_colors.inactive_tab
    else
      entry = tab_bar_colors.inactive_tab
    end
    fg = entry and entry.fg_color
  end
  -- fallback defaults when palette entry or sub-field is absent
  if not fg then
    fg = tab.is_active and '#edecee' or (hover and '#9b9b9b' or '#6b6b6b')
  end
  return {
    { Attribute = { Intensity = intensity } },
    { Foreground = { Color = fg } },
    { Text = ' ' .. text .. ' ' },
  }
end)

wezterm.on('window-resized', function(window, _)
  local dims = window:get_dimensions()
  update_window_config(window, dims.is_full_screen)
end)

wezterm.on('kaku-launch-lazygit', function(window, pane)
  launch_lazygit(window, pane)
end)

wezterm.on('kaku-ai-apply-last-fix', function(window, pane)
  pane = resolve_active_pane(window, pane)
  if not pane then
    return
  end

  local pane_id_ok, pane_id_value = pcall(function()
    return pane:pane_id()
  end)
  if not pane_id_ok or not pane_id_value then
    inject_ai_status(pane, "Unable to access the active pane.")
    return
  end

  local pane_state = ai_fix_state_by_pane[tostring(pane_id_value)]
  local suggestion = pane_state and pane_state.suggestion or nil
  local command = suggestion and sanitize_suggested_command(suggestion.command or "") or ""
  if command == "" then
    inject_ai_status(pane, "No quick fix command is available.")
    return
  end

  local send_text = command .. "\n"
  local toast_message = "Applied suggested command."
  if is_dangerous_command(command) then
    send_text = command
    toast_message = "Suggested command pasted only. Please review before running."
  end

  local sent_ok = pcall(function()
    pane:send_text("\x15" .. send_text)
  end)
  if not sent_ok then
    inject_ai_status(pane, "Unable to send the suggested command.")
    return
  end

  if is_dangerous_command(command) then
    inject_ai_status(pane, toast_message)
  end
end)

wezterm.on('user-var-changed', function(window, pane, name, value)
  ai_debug_log("user-var-changed name=" .. tostring(name) .. " value=" .. tostring(value))
  if name == "kaku_last_cmd" then
    if not pane then
      return
    end

    local pane_id_ok, pane_id_value = pcall(function()
      return pane:pane_id()
    end)
    if not pane_id_ok or not pane_id_value then
      return
    end

    local pane_id = tostring(pane_id_value)
    local pane_state = ai_fix_state_by_pane[pane_id]
    if not pane_state or not pane_state.inflight then
      return
    end

    pane_state.inflight = false
    pane_state.pending_job_id = nil
    clear_ai_fix_suggestion_state(pane_state)
    ai_debug_log("user-var-changed cancelled inflight ai fix pane_id=" .. pane_id)
    return
  end

  if name ~= "kaku_last_exit_code" then
    return
  end
  if not pane then
    ai_debug_log("user-var-changed ignored no pane")
    return
  end

  local exit_code = tonumber(value)
  if not exit_code then
    ai_debug_log("user-var-changed ignored invalid exit_code")
    return
  end
  if exit_code == 0 then
    ai_debug_log("user-var-changed ignored exit_code=0")
    return
  end
  if exit_code == 130 then
    ai_debug_log("user-var-changed ignored exit_code=130")
    return
  end

  local pane_id_ok, pane_id_value = pcall(function()
    return pane:pane_id()
  end)
  if not pane_id_ok or not pane_id_value then
    ai_debug_log("user-var-changed ignored pane_id unavailable")
    return
  end
  local pane_id = tostring(pane_id_value)
  local pane_state = ai_fix_state_by_pane[pane_id] or {}
  ai_fix_state_by_pane[pane_id] = pane_state

  if pane_state.inflight then
    ai_debug_log("user-var-changed skipped inflight pane_id=" .. pane_id)
    return
  end

  local vars_ok, vars = pcall(function()
    return pane:get_user_vars()
  end)
  if not vars_ok or type(vars) ~= "table" then
    ai_debug_log("user-var-changed ignored no user vars")
    return
  end

  local failed_command = trim_surrounding_whitespace(vars.kaku_last_cmd or "")
  if failed_command == "" then
    ai_debug_log("user-var-changed ignored missing kaku_last_cmd")
    return
  end
  ai_debug_log("user-var-changed failed_command=" .. failed_command .. " exit=" .. tostring(exit_code))

  local signature = failed_command .. "\0" .. tostring(exit_code)
  local now = now_secs()
  if pane_state.last_signature == signature and (now - (pane_state.last_seen_at or 0)) <= 1 then
    ai_debug_log("user-var-changed skipped duplicate signature")
    return
  end
  pane_state.last_signature = signature
  pane_state.last_seen_at = now

  clear_ai_fix_suggestion_state(pane_state)
  pane_state.failed_command = failed_command
  pane_state.exit_code = exit_code

  refresh_ai_fix_settings()
  if not ai_fix_enabled then
    ai_debug_log("user-var-changed ai_fix disabled after refresh")
    return
  end

  if not ai_fix_api_key or ai_fix_api_key == "" then
    ai_debug_log("user-var-changed missing api key after refresh")
    return
  end

  pane_state.inflight = true
  pane_state.pending_job_id = nil
  show_ai_loading_toast(window, pane)

  local cwd = pane_cwd(pane)
  local git_branch = detect_git_branch(cwd)
  local ok, err = request_ai_fix_async(window, pane, pane_id, failed_command, exit_code, cwd, git_branch)
  if not ok then
    pane_state.inflight = false
    pane_state.pending_job_id = nil
    clear_ai_fix_suggestion_state(pane_state)
    ai_debug_log("user-var-changed ai request start failed err=" .. tostring(err))
    inject_ai_status(pane, "Could not analyze this error right now.")
  end
end)

wezterm.on('update-right-status', function(window, pane)
  maybe_show_lazygit_hint(window, pane)

  local dims = window:get_dimensions()
  if not dims.is_full_screen then
    window:set_right_status('')
    return
  end

  local clock_icon = wezterm.nerdfonts.md_clock_time_four_outline
    or wezterm.nerdfonts.md_clock_outline
    or ''
  local text = wezterm.strftime('%H:%M')
  if clock_icon ~= '' then
    window:set_right_status(wezterm.format({
      { Foreground = { Color = '#6b6b6b' } },
      { Text = ' ' .. clock_icon .. ' ' .. text .. ' ' },
    }))
    return
  end
  window:set_right_status(wezterm.format({
    { Foreground = { Color = '#6b6b6b' } },
    { Text = ' ' .. text .. ' ' },
  }))
end)

-- ===== Font =====
-- System default CJK font (PingFang SC on macOS); let the system pick the best match.
config.font = wezterm.font_with_fallback({
  { family = 'JetBrains Mono', weight = 'Regular' },
  -- Omit explicit CJK font; macOS selects the best one based on locale.
  'Apple Color Emoji',
})

config.font_rules = {
  -- Prevent thin weight: use Regular instead of Light for Half intensity
  {
    intensity = 'Half',
    font = wezterm.font_with_fallback({
      { family = 'JetBrains Mono', weight = 'Regular' },
    }),
  },
  -- Normal italic: disable real italics (keep upright)
  {
    intensity = 'Normal',
    italic = true,
    font = wezterm.font_with_fallback({
      { family = 'JetBrains Mono', weight = 'Regular', italic = false },
    }),
  },
  -- Bold: use Medium weight instead of Heavy
  {
    intensity = 'Bold',
    font = wezterm.font_with_fallback({
      { family = 'JetBrains Mono', weight = 'Medium' },
    }),
  },
}

config.bold_brightens_ansi_colors = false
-- Auto-adjust font size based on screen DPI.
-- Retina (>=150 DPI): 17px, low-resolution external displays (<150 DPI): 15px.
local function get_font_size()
  local success, screens = pcall(function()
    return wezterm.gui.screens()
  end)
  if success and screens and screens.main then
    local dpi = screens.main.effective_dpi or 72
    if dpi < 150 then
      return 15.0  -- Low-resolution external display
    end
  end
  return 17.0  -- Retina default
end

config.font_size = get_font_size()
config.line_height = 1.28
config.cell_width = 1.0
config.harfbuzz_features = { 'calt=0', 'clig=0', 'liga=0' }
config.use_cap_height_to_scale_fallback_fonts = false

config.custom_block_glyphs = true
config.unicode_version = 14

local _, in_app_bundle = wezterm.executable_dir:gsub('MacOS/?$', 'Resources')
if in_app_bundle > 0 then
  config.term = 'kaku'
end

-- ===== Cursor =====
config.default_cursor_style = 'BlinkingBar'
config.cursor_thickness = '2px'
config.cursor_blink_rate = 500
-- Sharp on/off blink without fade animation (like a standard terminal).
config.cursor_blink_ease_in = 'Constant'
config.cursor_blink_ease_out = 'Constant'

-- ===== Scrollback =====
config.scrollback_lines = 10000

-- ===== Mouse =====
config.selection_word_boundary = ' \t\n{}[]()"\'-'  -- Smart selection boundaries

-- ===== Window =====
config.window_padding = {
  left = '40px',
  right = '40px',
  top = '70px',
  bottom = '30px',
}

config.initial_cols = 110
config.initial_rows = 22
-- Mitigate high GPU usage on macOS 26.x by disabling window shadow.
config.window_decorations = "INTEGRATED_BUTTONS|RESIZE|MACOS_FORCE_DISABLE_SHADOW"
config.window_frame = {
  font = wezterm.font({ family = 'JetBrains Mono', weight = 'Regular' }),
  font_size = 13.0,
  active_titlebar_bg = '#15141b',
  inactive_titlebar_bg = '#15141b',
}

config.window_close_confirmation = 'NeverPrompt'
config.window_background_opacity = 1.0
config.text_background_opacity = 1.0

-- ===== Tab Bar =====
config.enable_tab_bar = true
config.tab_bar_at_bottom = true
config.use_fancy_tab_bar = false
config.tab_max_width = 32
config.hide_tab_bar_if_only_one_tab = true
config.show_tab_index_in_tab_bar = true
config.show_new_tab_button_in_tab_bar = false

-- ===== Color Scheme =====
local kaku_theme = {
  -- Background
  foreground = '#edecee',
  background = '#15141b',

  -- Cursor
  cursor_bg = '#a277ff',
  cursor_fg = '#15141b',
  cursor_border = '#a277ff',

  -- Selection
  selection_bg = '#29263c',
  selection_fg = 'none',

  -- Normal colors (ANSI 0-7)
  ansi = {
    '#110f18',  -- black
    '#ff6767',  -- red
    '#61ffca',  -- green
    '#ffca85',  -- yellow
    '#a277ff',  -- blue
    '#a277ff',  -- magenta
    '#61ffca',  -- cyan
    '#edecee',  -- white
  },

  -- Bright colors (ANSI 8-15)
  brights = {
    '#4d4d4d',  -- bright black
    '#ff6767',  -- bright red
    '#61ffca',  -- bright green
    '#ffca85',  -- bright yellow
    '#a277ff',  -- bright blue
    '#a277ff',  -- bright magenta
    '#61ffca',  -- bright cyan
    '#edecee',  -- bright white
  },

  -- Split separator color (increased contrast for better visibility)
  split = '#3d3a4f',

  -- Tab bar colors
  tab_bar = {
    background = '#15141b',
    inactive_tab_edge = '#15141b',

    active_tab = {
      bg_color = '#29263c',
      fg_color = '#edecee',
      intensity = 'Bold',
      underline = 'None',
      italic = false,
      strikethrough = false,
    },

    inactive_tab = {
      bg_color = '#15141b',
      fg_color = '#6b6b6b',
      intensity = 'Normal',
    },

    inactive_tab_hover = {
      bg_color = '#1f1d28',
      fg_color = '#9b9b9b',
      italic = false,
    },

    new_tab = {
      bg_color = '#15141b',
      fg_color = '#6b6b6b',
    },

    new_tab_hover = {
      bg_color = '#1f1d28',
      fg_color = '#9b9b9b',
    },
  },
}

config.color_schemes = config.color_schemes or {}
config.color_schemes['Kaku Theme'] = kaku_theme
if not config.color_scheme then
  config.color_scheme = 'Kaku Theme'
end

-- ===== Shell =====
local user_shell = os.getenv('SHELL')
if user_shell and #user_shell > 0 then
  config.default_prog = { user_shell, '-l' }
else
  config.default_prog = { '/bin/zsh', '-l' }
end

-- ===== macOS Specific =====
-- Keep Left Option as Meta so Alt-based Vim/Neovim keybindings work reliably.
config.send_composed_key_when_left_alt_is_pressed = false
-- Keep Right Option available for composing locale/symbol characters.
config.send_composed_key_when_right_alt_is_pressed = true
config.native_macos_fullscreen_mode = true
config.quit_when_all_windows_are_closed = false

-- ===== Key Bindings =====
config.keys = {
  -- Cmd+K: clear screen + scrollback
  {
    key = 'k',
    mods = 'CMD',
    action = wezterm.action.Multiple({
      wezterm.action.SendKey({ key = 'l', mods = 'CTRL' }),
      wezterm.action.ClearScrollback('ScrollbackAndViewport'),
    }),
  },

  -- Compatibility: keep Cmd+R for existing muscle memory
  {
    key = 'r',
    mods = 'CMD',
    action = wezterm.action.Multiple({
      wezterm.action.SendKey({ key = 'l', mods = 'CTRL' }),
      wezterm.action.ClearScrollback('ScrollbackAndViewport'),
    }),
  },

  -- Cmd+Q: quit
  {
    key = 'q',
    mods = 'CMD',
    action = wezterm.action.QuitApplication,
  },

  -- Cmd+N: new window
  {
    key = 'n',
    mods = 'CMD',
    action = wezterm.action.SpawnWindow,
  },

  -- Cmd+W: close pane > close tab > hide app
  {
    key = 'w',
    mods = 'CMD',
    action = wezterm.action_callback(function(win, pane)
      local mux_win = win:mux_window()
      local tabs = mux_win and mux_win:tabs() or {}
      local current_tab = pane:tab()
      local panes = current_tab and current_tab:panes() or {}
      if #panes > 1 then
        win:perform_action(wezterm.action.CloseCurrentPane { confirm = false }, pane)
      else
        local should_close_tab = (#tabs > 1) or (#wezterm.mux.all_windows() > 1)
        if should_close_tab then
          win:perform_action(wezterm.action.CloseCurrentTab { confirm = false }, pane)
          return
        end
        win:perform_action(wezterm.action.HideApplication, pane)
      end
    end),
  },

  -- Cmd+Shift+W: close current tab
  {
    key = 'w',
    mods = 'CMD|SHIFT',
    action = wezterm.action.CloseCurrentTab({ confirm = false }),
  },

  -- Cmd+T: new tab
  {
    key = 't',
    mods = 'CMD',
    action = wezterm.action.SpawnTab('CurrentPaneDomain'),
  },

  -- Cmd+Shift+A: open Kaku AI settings in current pane
  {
    key = 'A',
    mods = 'CMD|SHIFT',
    action = wezterm.action.EmitEvent('run-kaku-ai-config'),
  },

  -- Cmd+Shift+E: apply latest Kaku Assistant suggestion for the active pane
  {
    key = 'E',
    mods = 'CMD|SHIFT',
    action = wezterm.action.EmitEvent('kaku-ai-apply-last-fix'),
  },

  -- Cmd+Shift+G: launch lazygit in current pane
  {
    key = 'G',
    mods = 'CMD|SHIFT',
    action = wezterm.action.EmitEvent('kaku-launch-lazygit'),
  },

  -- Cmd+Ctrl+F: toggle fullscreen
  {
    key = 'f',
    mods = 'CMD|CTRL',
    action = wezterm.action.ToggleFullScreen,
  },

  -- Cmd+M: minimize window
  {
    key = 'm',
    mods = 'CMD',
    action = wezterm.action.Hide,
  },

  -- Cmd+H: hide application
  {
    key = 'h',
    mods = 'CMD',
    action = wezterm.action.HideApplication,
  },

  -- Cmd+Equal/Minus/0: adjust font size
  {
    key = '=',
    mods = 'CMD',
    action = wezterm.action.IncreaseFontSize,
  },
  {
    key = '-',
    mods = 'CMD',
    action = wezterm.action.DecreaseFontSize,
  },
  {
    key = '0',
    mods = 'CMD',
    action = wezterm.action.ResetFontSize,
  },

  -- Alt+Left / Alt+Right: word jump
  {
    key = 'LeftArrow',
    mods = 'OPT',
    action = wezterm.action.SendKey({ key = 'b', mods = 'ALT' }),
  },
  {
    key = 'RightArrow',
    mods = 'OPT',
    action = wezterm.action.SendKey({ key = 'f', mods = 'ALT' }),
  },

  -- Cmd+Left / Cmd+Right: line start/end
  {
    key = 'LeftArrow',
    mods = 'CMD',
    action = wezterm.action.SendKey({ key = 'a', mods = 'CTRL' }),
  },
  {
    key = 'RightArrow',
    mods = 'CMD',
    action = wezterm.action.SendKey({ key = 'e', mods = 'CTRL' }),
  },

  -- Cmd+Backspace: delete to line start
  {
    key = 'Backspace',
    mods = 'CMD',
    action = wezterm.action.SendKey({ key = 'u', mods = 'CTRL' }),
  },

  -- Alt+Backspace: delete word
  {
    key = 'Backspace',
    mods = 'OPT',
    action = wezterm.action.SendKey({ key = 'w', mods = 'CTRL' }),
  },

  -- Cmd+D: vertical split
  {
    key = 'd',
    mods = 'CMD',
    action = wezterm.action.SplitHorizontal({ domain = 'CurrentPaneDomain' }),
  },

  -- Cmd+Shift+D: horizontal split
  {
    key = 'D',
    mods = 'CMD|SHIFT',
    action = wezterm.action.SplitVertical({ domain = 'CurrentPaneDomain' }),
  },

  -- Cmd+Shift+[ / ]: prev/next tab
  {
    key = '[',
    mods = 'CMD|SHIFT',
    action = wezterm.action.ActivateTabRelative(-1),
  },
  {
    key = ']',
    mods = 'CMD|SHIFT',
    action = wezterm.action.ActivateTabRelative(1),
  },

  -- Cmd+Option+Arrow: navigate between splits
  {
    key = 'LeftArrow',
    mods = 'CMD|OPT',
    action = wezterm.action.ActivatePaneDirection('Left'),
  },
  {
    key = 'RightArrow',
    mods = 'CMD|OPT',
    action = wezterm.action.ActivatePaneDirection('Right'),
  },
  {
    key = 'UpArrow',
    mods = 'CMD|OPT',
    action = wezterm.action.ActivatePaneDirection('Up'),
  },
  {
    key = 'DownArrow',
    mods = 'CMD|OPT',
    action = wezterm.action.ActivatePaneDirection('Down'),
  },

  -- Cmd+1~9: switch tab
  {
    key = '1',
    mods = 'CMD',
    action = wezterm.action.ActivateTab(0),
  },
  {
    key = '2',
    mods = 'CMD',
    action = wezterm.action.ActivateTab(1),
  },
  {
    key = '3',
    mods = 'CMD',
    action = wezterm.action.ActivateTab(2),
  },
  {
    key = '4',
    mods = 'CMD',
    action = wezterm.action.ActivateTab(3),
  },
  {
    key = '5',
    mods = 'CMD',
    action = wezterm.action.ActivateTab(4),
  },
  {
    key = '6',
    mods = 'CMD',
    action = wezterm.action.ActivateTab(5),
  },
  {
    key = '7',
    mods = 'CMD',
    action = wezterm.action.ActivateTab(6),
  },
  {
    key = '8',
    mods = 'CMD',
    action = wezterm.action.ActivateTab(7),
  },
  {
    key = '9',
    mods = 'CMD',
    action = wezterm.action.ActivateTab(8),
  },

  -- Cmd+Enter / Shift+Enter: newline without execute
  {
    key = 'Enter',
    mods = 'CMD',
    action = wezterm.action.SendString('\n'),
  },
  {
    key = 'Enter',
    mods = 'SHIFT',
    action = wezterm.action.SendString('\n'),
  },

  -- Cmd+Shift+Enter: Toggle Pane Zoom (Maximize active pane)
  {
    key = 'Enter',
    mods = 'CMD|SHIFT',
    action = wezterm.action.TogglePaneZoomState,
  },

  -- Cmd+Shift+S: Toggle split direction (horizontal <-> vertical)
  {
    key = 'S',
    mods = 'CMD|SHIFT',
    action = wezterm.action.TogglePaneSplitDirection,
  },

  -- Cmd+Ctrl+Arrows: Resize panes
  {
    key = 'LeftArrow',
    mods = 'CMD|CTRL',
    action = wezterm.action.AdjustPaneSize { 'Left', 5 },
  },
  {
    key = 'RightArrow',
    mods = 'CMD|CTRL',
    action = wezterm.action.AdjustPaneSize { 'Right', 5 },
  },
  {
    key = 'UpArrow',
    mods = 'CMD|CTRL',
    action = wezterm.action.AdjustPaneSize { 'Up', 5 },
  },
  {
    key = 'DownArrow',
    mods = 'CMD|CTRL',
    action = wezterm.action.AdjustPaneSize { 'Down', 5 },
  },


}

-- Copy on select (equivalent to Kitty's copy_on_select)
-- config.copy_on_select = false -- uncomment to disable copy and toast on selection
config.mouse_bindings = {
  {
    event = { Up = { streak = 1, button = 'Left' } },
    mods = 'NONE',
    action = wezterm.action.CompleteSelectionOrOpenLinkAtMouseCursor('ClipboardAndPrimarySelection'),
  },
  {
    event = { Up = { streak = 1, button = 'Left' } },
    mods = 'CMD',
    action = wezterm.action.OpenLinkAtMouseCursor,
  },
}

-- ===== Performance =====
config.enable_scroll_bar = false
config.front_end = 'WebGpu'
config.webgpu_power_preference = 'HighPerformance'
config.animation_fps = 60
config.max_fps = 60
config.status_update_interval = 1000

-- ===== Visuals & Splits =====
-- Split pane gap: gutter = 1 + 2*gap cells, giving ~40px padding on each side
config.split_pane_gap = 2

-- Inactive panes: No dimming (consistent background)
config.inactive_pane_hsb = {
  saturation = 1.0,
  brightness = 1.0,
}

-- Prevent accidental clicks when focusing panes
config.swallow_mouse_click_on_pane_focus = true
config.swallow_mouse_click_on_window_focus = true

-- ===== First Run Experience & Config Version Check =====
wezterm.on('gui-startup', function(cmd)
  local home = os.getenv("HOME")
  local current_version = 11  -- Update this when config changes

  local state_file = home .. "/.config/kaku/state.json"
  local legacy_version_file = home .. "/.config/kaku/.kaku_config_version"
  local legacy_geometry_file = home .. "/.config/kaku/.kaku_window_geometry"
  local is_first_run = false
  local needs_update = false

  local function ensure_state_dir()
    os.execute("mkdir -p " .. home .. "/.config/kaku")
  end

  local function write_state(version, geometry)
    ensure_state_dir()
    local state = {
      config_version = version,
    }
    if geometry and geometry.width and geometry.height then
      state.window_geometry = {
        width = geometry.width,
        height = geometry.height,
      }
    end

    local encoded = nil
    local ok, value = pcall(wezterm.json_encode, state)
    if ok and type(value) == "string" and value ~= "" then
      encoded = value
    end

    local wf = io.open(state_file, "w")
    if wf then
      if encoded then
        wf:write(encoded .. "\n")
      else
        -- Manual JSON fallback when json_encode is unavailable.
        -- Include geometry if present so state is not lost.
        if geometry and geometry.width and geometry.height then
          wf:write(string.format(
            '{\n  "config_version": %d,\n  "window_geometry": {\n    "width": %d,\n    "height": %d\n  }\n}\n',
            version, geometry.width, geometry.height))
        else
          wf:write(string.format('{\n  "config_version": %d\n}\n', version))
        end
      end
      wf:close()
    end
  end

  local function remove_legacy_files()
    os.remove(legacy_version_file)
    os.remove(legacy_geometry_file)
  end

  local function parse_legacy_geometry(raw)
    if not raw or raw == "" then
      return nil
    end

    local values = {}
    for number in raw:gmatch("%d+") do
      values[#values + 1] = tonumber(number)
    end

    if #values >= 4 then
      return {
        width = values[#values - 1],
        height = values[#values],
      }
    elseif #values >= 2 then
      return {
        width = values[1],
        height = values[2],
      }
    end

    return nil
  end

  local function migrate_legacy_state_if_needed()
    local existing_state = io.open(state_file, "r")
    if existing_state then
      existing_state:close()
      return
    end

    local legacy_version = nil
    local legacy_geometry = nil

    local lv = io.open(legacy_version_file, "r")
    if lv then
      legacy_version = tonumber(lv:read("*all"))
      lv:close()
    end

    local lg = io.open(legacy_geometry_file, "r")
    if lg then
      legacy_geometry = parse_legacy_geometry(lg:read("*all"))
      lg:close()
    end

    local has_legacy_markers = legacy_version ~= nil or legacy_geometry ~= nil

    if has_legacy_markers then
      write_state(legacy_version or current_version, legacy_geometry)
      remove_legacy_files()
    end
  end

  migrate_legacy_state_if_needed()

  local user_version = nil
  local state_file_exists = false
  local sf = io.open(state_file, "r")
  if sf then
    state_file_exists = true
    local raw_state = sf:read("*all")
    sf:close()
    if raw_state and raw_state ~= "" then
      local ok, state = pcall(wezterm.json_parse, raw_state)
      if ok and type(state) == "table" then
        user_version = tonumber(state.config_version)
      end
    end
  end

  if not state_file_exists then
    is_first_run = true
  elseif user_version == nil then
    -- Corrupted or manually edited state file: repair with safe defaults.
    write_state(current_version, nil)
    user_version = current_version
  elseif user_version < current_version then
    needs_update = true
  end

  if is_first_run then
    -- First run experience
    ensure_state_dir()

    local resource_dir = wezterm.executable_dir:gsub("MacOS/?$", "Resources")
    local first_run_script = resource_dir .. "/first_run.sh"

    -- Fallback for dev environment
    local f_script = io.open(first_run_script, "r")
    if not f_script then
      first_run_script = wezterm.executable_dir .. "/../../assets/shell-integration/first_run.sh"
    else
      f_script:close()
    end

    wezterm.mux.spawn_window {
      args = { 'bash', first_run_script },
      width = 106,
      height = 22,
    }
    return
  end

  if needs_update then
    -- Apply incremental config updates on version upgrades
    local resource_dir = wezterm.executable_dir:gsub("MacOS/?$", "Resources")
    local update_script = resource_dir .. "/check_config_version.sh"

    -- Fallback for dev environment
    local u_script = io.open(update_script, "r")
    if not u_script then
      update_script = wezterm.executable_dir .. "/../../assets/shell-integration/check_config_version.sh"
    else
      u_script:close()
    end

    wezterm.mux.spawn_window {
      args = { 'bash', update_script },
      width = 106,
      height = 22,
    }
    return
  end

  -- Normal startup
  if not cmd then
    wezterm.mux.spawn_window(cmd or {})
  end
end)

return config
