target extended-remote :3333

define hook-quit
    set confirm off
end

set backtrace limit 32

# detect unhandled exceptions, hard faults and panics
break DefaultHandler
break HardFault
break rust_begin_unwind

# *try* to stop at the user entry point (it might be gone due to inlining)
break main

load

# start the process but immediately halt the processor
stepi
