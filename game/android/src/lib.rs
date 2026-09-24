#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: game_core::AndroidApp) -> Result<(), Box<dyn std::error::Error>> {
	game_core::game_main(app)
}
