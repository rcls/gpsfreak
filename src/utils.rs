
pub fn unreachable() -> ! {
    #[cfg(target_os = "none")]
    unsafe {
        // This will cause a compiler error if not removed by the optimizer.
        unsafe extern "C" {fn nowayjose();}
        nowayjose();
    }
    panic!();
}

/// Cause a build time error if the condition fails and the code path is not
/// optimized out.  For test builds this is converted to a run-time check.
#[macro_export]
macro_rules! link_assert {
    ($e:expr) => { if !$e {$crate::utils::unreachable()} }
}
