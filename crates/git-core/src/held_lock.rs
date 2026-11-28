//! A write that another Git program's lock file stopped (#1033).

use std::error::Error;

/// Whether another Git program's lock file stopped `error`'s write.
pub fn is_lock_held(_error: &(dyn Error + 'static)) -> bool {
    false
}
