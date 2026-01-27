//! VFS path handling.

use crate::error::{Error, Result};

/// A validated VFS path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VfsPath {
    components: Vec<String>,
}

impl VfsPath {
    /// Parse a path string.
    ///
    /// Paths must be absolute (start with /).
    pub fn parse(path: &str) -> Result<Self> {
        if !path.starts_with('/') {
            return Err(Error::InvalidPath(
                "Path must be absolute (start with /)".to_string(),
            ));
        }

        let components: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Validate component names
        for component in &components {
            if component.contains('/') || component == "." || component == ".." {
                return Err(Error::InvalidPath(format!(
                    "Invalid path component: {}",
                    component
                )));
            }
        }

        Ok(Self { components })
    }

    /// Check if this is the root path.
    pub fn is_root(&self) -> bool {
        self.components.is_empty()
    }

    /// Get path components.
    pub fn components(&self) -> &[String] {
        &self.components
    }

    /// Get the parent path.
    pub fn parent(&self) -> Option<Self> {
        if self.is_root() {
            None
        } else {
            Some(Self {
                components: self.components[..self.components.len() - 1].to_vec(),
            })
        }
    }

    /// Get the file/directory name (last component).
    pub fn name(&self) -> Option<&str> {
        self.components.last().map(|s| s.as_str())
    }

    /// Join a child path component.
    pub fn join(&self, name: &str) -> Result<Self> {
        if name.contains('/') || name == "." || name == ".." || name.is_empty() {
            return Err(Error::InvalidPath(format!(
                "Invalid path component: {}",
                name
            )));
        }

        let mut components = self.components.clone();
        components.push(name.to_string());
        Ok(Self { components })
    }

    /// Convert to string representation.
    pub fn to_string(&self) -> String {
        if self.is_root() {
            "/".to_string()
        } else {
            format!("/{}", self.components.join("/"))
        }
    }

    /// Get the depth of this path.
    pub fn depth(&self) -> usize {
        self.components.len()
    }
}

impl std::fmt::Display for VfsPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_root() {
        let path = VfsPath::parse("/").unwrap();
        assert!(path.is_root());
        assert_eq!(path.to_string(), "/");
    }

    #[test]
    fn test_parse_simple() {
        let path = VfsPath::parse("/foo/bar").unwrap();
        assert!(!path.is_root());
        assert_eq!(path.components(), &["foo", "bar"]);
        assert_eq!(path.to_string(), "/foo/bar");
    }

    #[test]
    fn test_parse_trailing_slash() {
        let path = VfsPath::parse("/foo/bar/").unwrap();
        assert_eq!(path.components(), &["foo", "bar"]);
    }

    #[test]
    fn test_parse_relative_fails() {
        let result = VfsPath::parse("foo/bar");
        assert!(result.is_err());
    }

    #[test]
    fn test_parent() {
        let path = VfsPath::parse("/foo/bar/baz").unwrap();
        let parent = path.parent().unwrap();
        assert_eq!(parent.to_string(), "/foo/bar");

        let root_parent = VfsPath::parse("/").unwrap().parent();
        assert!(root_parent.is_none());
    }

    #[test]
    fn test_name() {
        let path = VfsPath::parse("/foo/bar.txt").unwrap();
        assert_eq!(path.name(), Some("bar.txt"));

        let root = VfsPath::parse("/").unwrap();
        assert_eq!(root.name(), None);
    }

    #[test]
    fn test_join() {
        let path = VfsPath::parse("/foo").unwrap();
        let joined = path.join("bar").unwrap();
        assert_eq!(joined.to_string(), "/foo/bar");
    }

    #[test]
    fn test_join_invalid() {
        let path = VfsPath::parse("/foo").unwrap();
        assert!(path.join("bar/baz").is_err());
        assert!(path.join("..").is_err());
        assert!(path.join(".").is_err());
        assert!(path.join("").is_err());
    }
}
