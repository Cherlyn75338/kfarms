pub mod accessors;
pub mod constraints;
pub mod consts;
pub mod macros;
pub mod math;
pub mod scope;
pub mod withdrawal_penalty;

#[cfg(any(test, debug_assertions))]
pub fn dev_assert(cond: bool, msg: &str) {
    debug_assert!(cond, "{}", msg);
}
