use std::{env, fs, path::{Path, PathBuf}};

fn main() {
	println!("cargo:rerun-if-changed=../assets");

	let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
	// OUT_DIR = target/<profile>/build/<pkg>-<hash>/out, walk up 3 to target/<profile>
	let target_dir = out_dir.ancestors().nth(3).expect("unexpected OUT_DIR layout").to_path_buf();

	let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
	let dst = target_dir.join("assets");

	copy_dir(&src, &dst).expect("failed to copy assets");
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), dst)?;
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}
