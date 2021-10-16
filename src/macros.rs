macro_rules! wrap_io_result {
    ($e:expr) => {{
        let result = unsafe { $e };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }};
}
