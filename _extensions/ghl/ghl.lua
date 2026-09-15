-- Quarto Lua Filter for GHL (Generalized Hypothesis Language)
-- Executes ```{ghl} code blocks and embeds outputs, data tables, and figures.

local session_history = {}

local function get_ghl_bin()
  local bin = os.getenv("GHL_BIN")
  if bin and bin ~= "" then
    return bin
  end

  local candidates = {
    "target/debug/ghl.exe",
    "target/release/ghl.exe",
    "../target/debug/ghl.exe",
    "../target/release/ghl.exe",
    "target/debug/ghl",
    "target/release/ghl",
    "ghl"
  }

  for _, path in ipairs(candidates) do
    local f = io.open(path, "r")
    if f then
      f:close()
      return path
    end
  end

  return "ghl"
end

function CodeBlock(block)
  -- Ignore blocks that are output blocks, error blocks, or previously echoed code blocks
  if block.classes:includes('ghl-echo') or block.classes:includes('ghl-output') or block.classes:includes('ghl-error') then
    return nil
  end

  -- Only handle blocks explicitly marked as {ghl} or ghl
  local is_ghl = false
  for _, c in ipairs(block.classes) do
    if c == "{ghl}" or c == "ghl" then
      is_ghl = true
      break
    end
  end

  if not is_ghl then
    return nil
  end

  local raw_text = block.text
  local echo = true
  local eval = true
  local results = "markup"
  local fig_cap = block.attributes['fig-cap']

  -- Parse Quarto #| directives
  local code_lines = {}
  for line in raw_text:gmatch("[^\r\n]+") do
    local opt_key, opt_val = line:match("^#|%s*([%w%-_]+):%s*(.*)$")
    if opt_key then
      opt_key = opt_key:lower()
      opt_val = opt_val:match("^%s*(.-)%s*$")
      if opt_key == "echo" then
        echo = (opt_val == "true")
      elseif opt_key == "eval" then
        eval = (opt_val == "true")
      elseif opt_key == "output" or opt_key == "results" then
        if opt_val == "false" or opt_val == "hide" then
          results = "hide"
        else
          results = "markup"
        end
      elseif opt_key == "fig-cap" then
        fig_cap = opt_val:gsub('^"', ''):gsub('"$', '')
      end
    else
      table.insert(code_lines, line)
    end
  end

  local current_code = table.concat(code_lines, "\n")
  local elements = {}

  -- 1. Echo code block in output document if enabled
  if echo then
    table.insert(elements, pandoc.CodeBlock(current_code, {class = "ghl ghl-echo"}))
  end

  if not eval then
    return elements
  end

  -- 2. Build cumulative script preserving session state across chunks
  local boundary_marker = "==GHL_CHUNK_OUTPUT_START=="
  local full_script = ""
  if #session_history > 0 then
    full_script = table.concat(session_history, "\n") .. "\n"
  end
  full_script = full_script .. 'println("' .. boundary_marker .. '");\n' .. current_code

  -- 3. Execute code via GHL CLI pipe
  local ghl_bin = get_ghl_bin()
  local success, raw_output = pcall(function()
    return pandoc.pipe(ghl_bin, {"run", "-q", "-"}, full_script)
  end)

  if not success then
    table.insert(elements, pandoc.CodeBlock(tostring(raw_output), {class = "ghl-error"}))
    return elements
  end

  -- Append current chunk to session history for subsequent chunks
  table.insert(session_history, current_code)

  -- 4. Extract only current chunk's output (after the boundary marker)
  local chunk_output = ""
  if raw_output then
    local marker_pos = raw_output:find(boundary_marker, 1, true)
    if marker_pos then
      chunk_output = raw_output:sub(marker_pos + #boundary_marker)
      -- Trim leading newline
      chunk_output = chunk_output:gsub("^\r?\n", "")
    else
      chunk_output = raw_output
    end
  end

  -- 5. Output console text / statistics
  if results ~= "hide" and chunk_output and chunk_output:match("%S") then
    table.insert(elements, pandoc.CodeBlock(chunk_output, {class = "ghl-output"}))
  end

  -- 6. Check for exported SVG/PNG figures mentioned in save(...) or |> save(...)
  for saved_path in current_code:gmatch('save%s*%(%s*["\']([^"\']+)["\']') do
    -- If path starts with target/, try both relative and ../relative
    local cand_paths = { saved_path, "../" .. saved_path }
    local found_path = nil
    for _, cp in ipairs(cand_paths) do
      local f = io.open(cp, "r")
      if f then
        f:close()
        found_path = cp
        break
      end
    end

    if found_path then
      local caption = fig_cap or "GHL Statistical Graphic"
      table.insert(elements, pandoc.Para({ pandoc.Image({ pandoc.Str(caption) }, found_path, caption) }))
    end
  end

  return elements
end
