#[derive(Debug, Clone)]
pub struct RecipeMediaModel {
	pub name: String,
	// Ejemplo: ["sm", "md", "lg"]
	pub target_size: Vec<MediaSizeModel>,
	// Ejemplo: "webp"
	pub format: String,
	pub radio: f64,
	pub priority: i32,
}

#[derive(Debug, Clone)]
pub struct MediaSizeModel {
	// Ejemplo: "sm", "md", "lg"
	pub size: String,
	pub width: i32,
	pub height: i32,
	// Ejemplo: "webp"
	pub format: String,
	pub priority: i32,
}

