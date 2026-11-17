//! Run the owning packages' regression suites against the exact source pins
//! consumed by this host. Test implementations remain in their source repos.
#[path = "../../../packages/lekalo-go-discovery/tests/discovery.rs"]
mod go;
#[path = "../../../packages/lekalo-rust-discovery/tests/discovery.rs"]
mod rust;
#[path = "../../../packages/lekalo-typescript-discovery/tests/discovery.rs"]
mod typescript;
