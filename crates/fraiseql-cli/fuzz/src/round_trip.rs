//! The property both targets check, as the issue states it: whatever `fraiseql compile`
//! accepts, the `schema.compiled.json` it writes loads where a server loads it.
//!
//! The compile runs exactly as `fraiseql compile <input>` does with no database
//! (`compile_for_fuzzing`), and its output goes to `CompiledSchema::from_json` with the
//! content hash enforced, as a server's boot does. The compiler re-checks only the schema
//! half before it returns; this checks the whole file it writes, sibling sections and hash
//! included.
//!
//! Every input is written as the only file of a directory of its own: an authoring IR's
//! project config is the `fraiseql.toml` beside it, and there is none. The process's
//! working directory is left alone, because libFuzzer resolves the corpus and artifact
//! paths it is given against it.

use std::{path::PathBuf, sync::OnceLock};

/// The directory inputs are written to, one per process.
fn workdir() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("fraiseql-cli-fuzz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create the input directory");
        dir
    })
}

/// Compile `text` as the file `file_name`; when the compile accepts it, the file it writes
/// must load.
pub fn check(file_name: &str, text: &str) {
    let path = workdir().join(file_name);
    std::fs::write(&path, text).expect("write the input");
    let input = path.to_str().expect("a UTF-8 input directory");
    let Ok(written) = fraiseql_cli::commands::compile::compile_for_fuzzing(input) else {
        return;
    };
    if let Err(e) = fraiseql_core::schema::CompiledSchema::from_json(&written, true) {
        panic!(
            "`fraiseql compile` accepted {file_name}, and a server refuses the \
             schema.compiled.json it writes: {e}\n--- input ---\n{text}\n--- written ---\n{written}"
        );
    }
}
