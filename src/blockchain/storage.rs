use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use super::block::Block;

/// Blocks are stored one JSON object per line, in height order.
pub fn load(path: &Path) -> io::Result<Vec<Block>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    fs::read_to_string(path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(io::Error::other))
        .collect()
}

pub fn append(path: &Path, block: &Block) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", serde_json::to_string(block)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::transaction::Address;

    #[test]
    fn appended_blocks_are_loaded_back() {
        let path = std::env::temp_dir().join(format!("ferrune-{}.jsonl", uuid::Uuid::new_v4()));
        assert!(load(&path).unwrap().is_empty());

        let genesis = Block::new(0, [0; 32], vec![], Address("a".into())).unwrap();
        let next = Block::new(1, genesis.hash, vec![], Address("a".into())).unwrap();
        append(&path, &genesis).unwrap();
        append(&path, &next).unwrap();

        let blocks = load(&path).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].hash, next.hash);
        fs::remove_file(path).unwrap();
    }
}
