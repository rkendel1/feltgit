pub mod state_history;

#[cfg(feature = "git-integration")]
pub mod csum_file;
#[cfg(feature = "git-integration")]
pub mod hash;
#[cfg(feature = "git-integration")]
pub mod loose;
#[cfg(feature = "git-integration")]
pub mod varint;
