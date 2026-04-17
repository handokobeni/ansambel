use crate::error::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;

pub fn write_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let file = File::create(&tmp)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, value)?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load_or_default<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    let content = fs::read_to_string(path)?;
    let value: T = serde_json::from_str(&content)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug, Default)]
    struct Sample {
        name: String,
        count: u32,
    }

    #[test]
    fn write_atomic_creates_file_with_content() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("data.json");
        let value = Sample {
            name: "ansambel".into(),
            count: 7,
        };

        write_atomic(&path, &value).unwrap();

        let loaded: Sample =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, value);
    }

    #[test]
    fn write_atomic_leaves_no_tmp_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("data.json");
        write_atomic(
            &path,
            &Sample {
                name: "x".into(),
                count: 1,
            },
        )
        .unwrap();
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn write_atomic_overwrites_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("data.json");
        std::fs::write(&path, b"stale").unwrap();

        write_atomic(
            &path,
            &Sample {
                name: "new".into(),
                count: 2,
            },
        )
        .unwrap();

        let loaded: Sample =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.name, "new");
    }

    #[test]
    fn write_atomic_creates_parent_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested/deep/data.json");

        write_atomic(
            &path,
            &Sample {
                name: "a".into(),
                count: 3,
            },
        )
        .unwrap();

        assert!(path.exists());
    }

    #[test]
    fn load_or_default_returns_default_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("missing.json");
        let loaded: Sample = load_or_default(&path).unwrap();
        assert_eq!(
            loaded,
            Sample {
                name: String::new(),
                count: 0
            }
        );
    }

    #[test]
    fn load_or_default_reads_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("data.json");
        let v = Sample {
            name: "z".into(),
            count: 9,
        };
        write_atomic(&path, &v).unwrap();

        let loaded: Sample = load_or_default(&path).unwrap();
        assert_eq!(loaded, v);
    }
}
