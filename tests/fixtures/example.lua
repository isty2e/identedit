local M = {
  offset = 1,
}

function M.process_data(value)
  if value > 0 then
    return value + M.offset
  end

  return 0
end

function helper(value)
  return value * 2
end

return M
