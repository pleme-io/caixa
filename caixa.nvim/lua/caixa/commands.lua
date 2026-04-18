-- :Caixa* user commands that wrap the `feira` CLI.

local M = {}

local function feira(cfg, args)
  local cmd = cfg.feira_cmd or "feira"
  if vim.fn.executable(cmd) ~= 1 then
    vim.notify(
      "feira binary not found — install via `nix run .#default` or PATH",
      vim.log.levels.WARN,
      { title = "caixa.nvim" }
    )
    return
  end
  local full = vim.list_extend({ cmd }, args or {})
  local out = vim.fn.system(full)
  if vim.v.shell_error ~= 0 then
    vim.notify(out, vim.log.levels.ERROR, { title = "feira " .. (args[1] or "") })
  else
    vim.notify(out, vim.log.levels.INFO, { title = "feira " .. (args[1] or "") })
  end
end

function M.setup(cfg)
  vim.api.nvim_create_user_command("FeiraLock", function() feira(cfg, { "lock" }) end, {})
  vim.api.nvim_create_user_command("FeiraBuild", function() feira(cfg, { "build" }) end, {})
  vim.api.nvim_create_user_command("FeiraNix", function() feira(cfg, { "nix" }) end, {})
  vim.api.nvim_create_user_command("FeiraFmt", function() feira(cfg, { "fmt" }) end, {})
  vim.api.nvim_create_user_command("FeiraLint", function() feira(cfg, { "lint" }) end, {})
  vim.api.nvim_create_user_command("FeiraResolve", function() feira(cfg, { "resolve" }) end, {})
  vim.api.nvim_create_user_command("FeiraPublish", function(o)
    feira(cfg, { "publish", o.args })
  end, { nargs = "?" })

  -- Keymaps under <leader>c — blackmatter-default prefix.
  local map = function(lhs, cmd) vim.keymap.set("n", lhs, cmd, { silent = true }) end
  map("<leader>cl", ":FeiraLock<CR>")
  map("<leader>cb", ":FeiraBuild<CR>")
  map("<leader>cf", ":FeiraFmt<CR>")
  map("<leader>ci", ":FeiraLint<CR>")
  map("<leader>cn", ":FeiraNix<CR>")
end

return M
