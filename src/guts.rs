//! This undocumented and unstable module is for use cases like the `bao` crate,
//! which need to traverse the BLAKE3 Merkle tree and work with chunk and parent
//! chaining values directly. There might be breaking changes to this module
//! between patch versions.
//!
//! We could stabilize something like this module in the future. If you have a
//! use case for it, please let us know by filing a GitHub issue.

pub const BLOCK_LEN: usize = 64;
pub const CHUNK_LEN: usize = 1024;

fn is_subtree(start_chunk: u64, len: u64) -> bool {
    const CHUNK_LEN_U64: u64 = CHUNK_LEN as u64;
    let chunks = len / CHUNK_LEN_U64 + (len % CHUNK_LEN_U64 != 0) as u64;
    let block_mask = chunks.next_power_of_two() - 1;
    start_chunk & block_mask == 0
}

/// Compute the hash of a subtree consisting of one or many chunks.
///
/// The range given by `start_chunk` and `len` must be a single subtree, i.e.
/// `is_subtree(start_chunk, len)` must be true. The `is_root` flag indicates
/// whether the subtree is the root of the tree.
///
/// Subtrees that start at a non zero chunk can not be the root.
pub fn hash_subtree(start_chunk: u64, data: &[u8], is_root: bool) -> crate::Hash {
    debug_assert!(is_subtree(start_chunk, data.len() as u64));
    debug_assert!(start_chunk == 0 || !is_root);
    let mut hasher = crate::Hasher::new_with_start_chunk(start_chunk);
    hasher.update(data);
    hasher.finalize_node(is_root)
}

/// Rayon parallel version of [`hash_block`].
#[cfg(feature = "rayon")]
pub fn hash_subtree_rayon(start_chunk: u64, data: &[u8], is_root: bool) -> crate::Hash {
    debug_assert!(is_subtree(start_chunk, data.len() as u64));
    debug_assert!(start_chunk == 0 || !is_root);
    let mut hasher = crate::Hasher::new_with_start_chunk(start_chunk);
    hasher.update_rayon(data);
    hasher.finalize_node(is_root)
}

#[derive(Clone, Debug)]
pub struct ChunkState(crate::ChunkState);

impl ChunkState {
    // Currently this type only supports the regular hash mode. If an
    // incremental user needs keyed_hash or derive_key, we can add that.
    pub fn new(chunk_counter: u64) -> Self {
        Self(crate::ChunkState::new(
            crate::IV,
            chunk_counter,
            0,
            crate::platform::Platform::detect(),
        ))
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub fn update(&mut self, input: &[u8]) -> &mut Self {
        self.0.update(input);
        self
    }

    pub fn finalize(&self, is_root: bool) -> crate::Hash {
        let output = self.0.output();
        if is_root {
            output.root_hash()
        } else {
            output.chaining_value().into()
        }
    }
}

// As above, this currently assumes the regular hash mode. If an incremental
// user needs keyed_hash or derive_key, we can add that.
pub fn parent_cv(
    left_child: &crate::Hash,
    right_child: &crate::Hash,
    is_root: bool,
) -> crate::Hash {
    let output = crate::parent_node_output(
        left_child.as_bytes(),
        right_child.as_bytes(),
        crate::IV,
        0,
        crate::platform::Platform::detect(),
    );
    if is_root {
        output.root_hash()
    } else {
        output.chaining_value().into()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_chunk() {
        assert_eq!(
            crate::hash(b"foo"),
            ChunkState::new(0).update(b"foo").finalize(true)
        );
    }

    #[test]
    fn test_parents() {
        let mut hasher = crate::Hasher::new();
        let mut buf = [0; crate::CHUNK_LEN];

        buf[0] = 'a' as u8;
        hasher.update(&buf);
        let chunk0_cv = ChunkState::new(0).update(&buf).finalize(false);

        buf[0] = 'b' as u8;
        hasher.update(&buf);
        let chunk1_cv = ChunkState::new(1).update(&buf).finalize(false);

        hasher.update(b"c");
        let chunk2_cv = ChunkState::new(2).update(b"c").finalize(false);

        let parent = parent_cv(&chunk0_cv, &chunk1_cv, false);
        let root = parent_cv(&parent, &chunk2_cv, true);
        assert_eq!(hasher.finalize(), root);
    }

    #[test]
    fn test_hash_block() {
        assert_eq!(crate::hash(b"foo"), hash_subtree(0, b"foo", true));

        assert_eq!(is_subtree(4, 1024 * 4 - 1), true);
        assert_eq!(is_subtree(1, 1024 * 4), false);
    }
}
