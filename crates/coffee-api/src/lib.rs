//! Serde DTOs of the ROAST/TTY backend's `/v1` HTTP API.
//!
//! Placeholder: the DTOs land here, hand-written to match `packages/contract`'s fixtures.
//! This crate must not depend on nsh.

/// The API version prefix every backend route lives under.
pub const API_PREFIX: &str = "/v1";

#[cfg(test)]
mod tests {
  #[test]
  fn prefix_is_v1() {
    assert_eq!(super::API_PREFIX, "/v1");
  }
}
