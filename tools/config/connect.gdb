# Stop before user commands when the server connection failed.
if $_thread == 0
    quit 1
end
monitor halt
maintenance flush register-cache
