use xxhash_rust::xxh3::Xxh3;

/// The same on every platform and Rust version, so that it can name files and cache entries that
/// outlive a toolchain upgrade. Each part is length-prefixed, so different splits hash apart.
pub fn stable_hash(parts: &[&[u8]]) -> u64 {
    let mut state = Xxh3::new();
    for part in parts {
        state.update(&(part.len() as u64).to_le_bytes());
        state.update(part);
    }
    state.digest()
}
