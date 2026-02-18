#!/usr/bin/env fish

function process_data
  set value $argv[1]
  math $value + 1
end

function helper
  echo helper
end

if test "$argv[1]" = "run"
  process_data 3
end
