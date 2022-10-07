use std::path::Path;

use eyre::Context;

pub fn create_dir_if_not_exists<P: AsRef<Path>>(path: P) -> eyre::Result<()> {
    let path: &Path = path.as_ref();

    if !path.exists() {
        std::fs::create_dir(path)
            .wrap_err_with(|| format!("Error creating directory {:?}", path))?;
    }

    Ok(())
}
