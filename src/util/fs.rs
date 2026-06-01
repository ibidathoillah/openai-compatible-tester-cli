use std::path::Path;

use anyhow::Context;

pub fn write_text(path: &Path, content: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    std::fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::write_text;

    #[test]
    fn writes_text_and_creates_parent_directories() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nested").join("file.txt");

        write_text(&path, "hello").unwrap();

        assert_eq!(std::fs::read_to_string(path).unwrap(), "hello");
    }
}
