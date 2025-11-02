// Simple test to verify Phase 1 ROI implementation
use gstreamed_ort::tui::roi::{RoiZone, RoiBBox, save_zones, load_zones};

fn main() {
    println!("=== Phase 1 ROI Zone Test ===\n");
    
    // Test 1: Create zones
    println!("1. Creating zones...");
    let mut zones = vec![
        RoiZone::new("Parking Area".to_string()),
        RoiZone::new_with_bbox(
            "Entrance".to_string(),
            RoiBBox::new(0.1, 0.1, 0.3, 0.3),
        ),
        RoiZone::new_with_bbox(
            "Exit".to_string(),
            RoiBBox::new(0.7, 0.7, 0.9, 0.9),
        ),
    ];
    println!("   Created {} zones", zones.len());
    for (i, zone) in zones.iter().enumerate() {
        println!("   Zone {}: {} [{}] ({:.2},{:.2}) -> ({:.2},{:.2})", 
            i+1, zone.name, zone.id, 
            zone.bbox.xmin, zone.bbox.ymin,
            zone.bbox.xmax, zone.bbox.ymax);
    }
    
    // Test 2: Validate and clamp
    println!("\n2. Testing validation...");
    let mut bad_zone = RoiZone::new_with_bbox(
        "Invalid".to_string(),
        RoiBBox { xmin: -0.5, ymin: 1.5, xmax: 2.0, ymax: 0.3 },
    );
    println!("   Before: ({:.2},{:.2}) -> ({:.2},{:.2})", 
        bad_zone.bbox.xmin, bad_zone.bbox.ymin,
        bad_zone.bbox.xmax, bad_zone.bbox.ymax);
    bad_zone.validate_and_clamp();
    println!("   After:  ({:.2},{:.2}) -> ({:.2},{:.2})", 
        bad_zone.bbox.xmin, bad_zone.bbox.ymin,
        bad_zone.bbox.xmax, bad_zone.bbox.ymax);
    
    // Test 3: Save zones
    println!("\n3. Saving zones to zones.json...");
    match save_zones(&zones) {
        Ok(_) => println!("   ✓ Saved successfully"),
        Err(e) => println!("   ✗ Error: {}", e),
    }
    
    // Test 4: Load zones
    println!("\n4. Loading zones from zones.json...");
    match load_zones() {
        Ok(loaded) => {
            println!("   ✓ Loaded {} zones", loaded.len());
            for (i, zone) in loaded.iter().enumerate() {
                println!("   Zone {}: {} [enabled: {}]", 
                    i+1, zone.name, zone.enabled);
            }
        }
        Err(e) => println!("   ✗ Error: {}", e),
    }
    
    // Test 5: Toggle zone
    println!("\n5. Testing zone enable/disable...");
    zones[0].enabled = false;
    println!("   Zone '{}' enabled: {}", zones[0].name, zones[0].enabled);
    
    // Test 6: Area calculation
    println!("\n6. Testing area calculation...");
    for zone in &zones {
        let area = zone.bbox.area();
        println!("   Zone '{}': {:.2}% of frame", zone.name, area * 100.0);
    }
    
    println!("\n=== All Phase 1 Tests Complete ===");
}
