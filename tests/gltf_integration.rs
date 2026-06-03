use std::path::Path;
mod common;

#[test]
fn test_load_and_reproject_gltf() {
    // 1. Attempt to initialize headless/software WGPU device.
    // If running in a context where even software rendering isn't available, we skip gracefully.
    let Some((device, layout)) = common::try_init_wgpu() else {
        println!("Skipping integration test: WGPU adapter not available in this environment.");
        return;
    };

    let path = Path::new("models/GlassVaseFlowers.glb");
    if !path.exists() {
        println!("Skipping integration test: test model path 'models/GlassVaseFlowers.glb' does not exist.");
        return;
    }

    // 2. Load the glTF document
    let mut doc = dcc_painter::mesh::load_gltf(&device, &layout, path)
        .expect("Failed to load glTF document");

    // 3. Perform basic sanity checks
    assert!(!doc.scenes.is_empty(), "Document should contain at least one scene");
    assert!(!doc.nodes.is_empty(), "Document should contain at least one node");

    // Check bounds calculations
    let bounds = doc.compute_bounds();
    assert!(bounds.is_some(), "Document should have non-empty bounds");
    let (min, max) = bounds.unwrap();
    assert!(min.x <= max.x && min.y <= max.y && min.z <= max.z, "Invalid bounding box dimensions");

    // 4. Test reprojecting UV layouts
    let settings = dcc_painter::mesh::ImportSettings {
        seams_option: dcc_painter::mesh::SeamsOption::RecomputeAll,
        margin_size: dcc_painter::mesh::MarginSize::Medium,
        island_orientation: dcc_painter::mesh::IslandOrientation::AlignWith3DMesh,
    };

    doc.recompute_uvs(&settings, &device);

    // Verify bounds remain intact after reprojection
    let bounds_after = doc.compute_bounds().unwrap();
    assert!((bounds_after.0 - min).length() < 1e-3, "Bounds min shifted during UV reprojection");
    assert!((bounds_after.1 - max).length() < 1e-3, "Bounds max shifted during UV reprojection");
}
