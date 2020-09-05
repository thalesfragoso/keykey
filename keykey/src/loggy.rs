#[cfg(feature = "log")]
macro_rules! log {
    ($($t:tt)*) => {{ rtt_target::rprintln!($($t)*); }};
}

#[cfg(feature = "log")]
macro_rules! init_log {
    () => {{
        rtt_target::rtt_init_print!();
    }};
}

#[cfg(not(feature = "log"))]
macro_rules! log {
    ($($t:tt)*) => {{ format_args!($($t)*); }};
}

#[cfg(not(feature = "log"))]
macro_rules! init_log {
    () => {{
        ();
    }};
}
