pub use include_dir::{Dir, File, include_dir};

pub static DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/dist");
