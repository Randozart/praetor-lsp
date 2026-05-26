/// Binary analysis module: lift binaries to IR, extract CFG facts, detect anti-patterns,
/// apply surgical patches, and verify CFG topology equivalence.
pub mod lift;
pub mod facts;
pub mod patterns;
pub mod patch;
pub mod verify;