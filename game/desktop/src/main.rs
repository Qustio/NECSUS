fn main() -> Result<(), Box<dyn std::error::Error>> {
	#[cfg(target_os = "android")]
	std::unreachable!("desktop crate should not be build for android");
	#[cfg(not(target_os = "android"))]
	game_core::game_main()
}