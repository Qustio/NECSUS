use std::{env, error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
	let global_session = slang::GlobalSession::new().ok_or("Slang missing")?;
	let out_dir = PathBuf::from(env::var("OUT_DIR")?);
	let target_profile_dir = out_dir.ancestors().nth(3).unwrap();
	fs::create_dir_all(target_profile_dir.join("shaders"))?;
	let search_path = std::ffi::CString::new("shaders").unwrap();
	
	let session_options = slang::CompilerOptions::default()
		.matrix_layout_column(true);
	let target_desc = slang::TargetDesc::default()
		.format(slang::CompileTarget::Spirv)
		.profile(global_session.find_profile("glsl_450"));
	let targets = [target_desc];
	let search_paths = [search_path.as_ptr()];
	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.search_paths(&search_paths)
		.options(&session_options);
	let session = global_session.create_session(&session_desc).unwrap();
	for module_name in ["back.slang", "main.slang"]{
		let module = session.load_module(module_name)?;
		let entry_point_v = module.find_entry_point_by_name("vertMain").ok_or("No entrypoint found")?;
		let entry_point_f = module.find_entry_point_by_name("fragMain").ok_or("No entrypoint found")?;

		let program = session
			.create_composite_component_type(&[module.into(), entry_point_v.into(), entry_point_f.into()])
			.unwrap();

		let linked = program.link()?;
		let code = linked.target_code(0)?;
		let spirv: &[u8] = code.as_slice();

		println!("cargo::warning={:?}", target_profile_dir.join("shaders").join(module_name));
		fs::write(target_profile_dir.join("shaders").join(module_name), spirv)?;

		println!("cargo:rerun-if-changed=shaders/vertex.slang");
	}
	Ok(())
}