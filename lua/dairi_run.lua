local api = vim.api
local cmd = vim.cmd
local bo = vim.bo
local fn = vim.fn

local dairi_client = require("dairi")

local M = {}

local config = {
	cmds = {
		julia = "julia",
	},
}

function M.setup(user_options)
	config = vim.tbl_deep_extend("force", config, user_options)
end

local function win_info()
	local info = {}
	info.width = math.ceil(api.nvim_get_option("columns"))
	info.height = math.ceil(api.nvim_get_option("lines"))
	return info
end

local function result_window_opts()
	local w_padding = 6
	local h_padding = 6
	local info = win_info()
	local width = math.max(info.width - (w_padding * 2), 1)
	local height = math.max(info.height - (h_padding * 2), 1)
	local row = math.ceil((info.height - height) / 2)
	local col = w_padding

	local opts = {
		style = "minimal",
		relative = "win",
		border = "solid",
		width = width,
		height = height,
		row = row,
		col = col,
		noautocmd = true,
	}
	return opts
end

local function find_result_window()
	for _, win_id in ipairs(api.nvim_list_wins()) do
		local bufnr = api.nvim_win_get_buf(win_id)

		local ft = fn.getbufvar(bufnr, "&filetype")
		if ft == "dairi_result" then
			return win_id
		end
	end
	return nil
end

local function get_buffer_contents(bufnr)
	if bufnr == nil then
		bufnr = 0
	end

	local content = api.nvim_buf_get_lines(bufnr, 0, -1, false)
	return table.concat(content, "\n")
end

local function close_window_if_exists()
	local found_win_id = find_result_window()
	if found_win_id ~= nil then
		local bufnr = api.nvim_win_get_buf(found_win_id)

		api.nvim_buf_delete(bufnr, { force = true })
	end
end

local function create_result_buffer()
	close_window_if_exists()

	local win_opts = result_window_opts()
	local bufnr = api.nvim_create_buf(false, true)
	api.nvim_buf_set_option(bufnr, "filetype", "dairi_result")
	api.nvim_buf_set_keymap(bufnr, "n", "<Esc><Esc>", "<C-n>:lua vim.api.nvim_win_close(0, true)<CR>", {

		silent = true,
	})

	local win = api.nvim_open_win(bufnr, true, win_opts)
	api.nvim_win_set_option(win, "winblend", 10)
	return bufnr
end

local function run_dairi_cmd(cmd_name, input)
	return dairi_client.run_cmd(cmd_name, input)
end

local function output_contents(bufnr, cmd_name, contents)
	local output = {}
	table.insert(output, "result of " .. cmd_name .. ":")
	for each in contents:gmatch("[^\r\n]+") do
		table.insert(output, each)
	end

	api.nvim_buf_set_text(bufnr, 0, 0, 0, 0, output)
end

local function get_cmd_by_filetype(file_type)
	return config.cmds[file_type]
end

function M.close_window_if_exists()
	close_window_if_exists()
end

function M.run(no_output)
	local filetype = bo.filetype
	local cmd_name = get_cmd_by_filetype(filetype)
	if cmd_name ~= nil then
		local input = get_buffer_contents(0)
		local result = run_dairi_cmd(cmd_name, input)

		if no_output ~= true then
			local bufnr = create_result_buffer()
			get_buffer_contents(bufnr)
			output_contents(bufnr, cmd_name, result)
		end
	else
		error("no cmd defined for ft:[" .. filetype .. "]")
	end
end

return M
