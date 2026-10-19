/// Marks the process non-dumpable. Decrypted secrets then stay out of core
/// dumps, and other processes running as the same user can't attach a
/// debugger or read this process's memory through `/proc`. Linux resets the
/// flag on exec, so the command `hush run` starts is unaffected.
pub fn no_dumps() {
    #[cfg(target_os = "linux")]
    // SAFETY: PR_SET_DUMPABLE takes plain integer arguments and touches no memory.
    unsafe {
        libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
    }
}
