//! Test for zone adjustment boundary constraints fix
//! 
//! Tests the fix for the zone adjustment issue where:
//! - Top edge decrease wasn't working
//! - Left edge increase/decrease wasn't working  
//! - Bottom edge decrease wasn't working

use gstreamed_ort::tui::app::App;
use gstreamed_ort::tui::roi::{RoiZone, RoiBBox, MIN_ZONE_SIZE};

fn create_test_zone() -> RoiZone {
    RoiZone::new_with_bbox(
        "Test Zone".to_string(),
        RoiBBox {
            xmin: 0.3,
            ymin: 0.3,
            xmax: 0.7,
            ymax: 0.7,
        },
    )
}

#[test]
fn test_left_edge_decrease() {
    // Ctrl+Left: Move left edge LEFT (decrease xmin)
    let mut app = App::new();
    app.zone_draft = Some(create_test_zone());
    
    // Decrease xmin by 0.05 (moving left edge left)
    app.adjust_zone_bbox(-0.05, 0.0, 0.0, 0.0);
    
    let zone = app.zone_draft.as_ref().unwrap();
    assert_eq!(zone.bbox.xmin, 0.25);
    assert_eq!(zone.bbox.xmax, 0.7);
    println!("✓ Left edge decreased: xmin=0.25, xmax=0.7");
}

#[test]
fn test_left_edge_increase_with_limit() {
    // Ctrl+Right: Move left edge RIGHT (increase xmin, but stop near xmax)
    let mut app = App::new();
    app.zone_draft = Some(create_test_zone());
    
    // Try to increase xmin way beyond xmax
    app.adjust_zone_bbox(0.5, 0.0, 0.0, 0.0); // Try to set xmin=0.8
    
    let zone = app.zone_draft.as_ref().unwrap();
    // Should be clamped to xmax - MIN_ZONE_SIZE
    let expected_xmin = 0.7 - MIN_ZONE_SIZE;
    assert!((zone.bbox.xmin - expected_xmin).abs() < 0.001);
    assert_eq!(zone.bbox.xmax, 0.7);
    println!("✓ Left edge constrained: xmin={:.3}, xmax=0.7 (gap={:.3})", 
             zone.bbox.xmin, zone.bbox.xmax - zone.bbox.xmin);
}

#[test]
fn test_top_edge_decrease() {
    // Ctrl+Up: Move top edge UP (decrease ymin)
    let mut app = App::new();
    app.zone_draft = Some(create_test_zone());
    
    // Decrease ymin by 0.05 (moving top edge up)
    app.adjust_zone_bbox(0.0, -0.05, 0.0, 0.0);
    
    let zone = app.zone_draft.as_ref().unwrap();
    assert_eq!(zone.bbox.ymin, 0.25);
    assert_eq!(zone.bbox.ymax, 0.7);
    println!("✓ Top edge decreased: ymin=0.25, ymax=0.7");
}

#[test]
fn test_top_edge_increase_with_limit() {
    // Ctrl+Down: Move top edge DOWN (increase ymin, but stop near ymax)
    let mut app = App::new();
    app.zone_draft = Some(create_test_zone());
    
    // Try to increase ymin way beyond ymax
    app.adjust_zone_bbox(0.0, 0.5, 0.0, 0.0); // Try to set ymin=0.8
    
    let zone = app.zone_draft.as_ref().unwrap();
    // Should be clamped to ymax - MIN_ZONE_SIZE
    let expected_ymin = 0.7 - MIN_ZONE_SIZE;
    assert!((zone.bbox.ymin - expected_ymin).abs() < 0.001);
    assert_eq!(zone.bbox.ymax, 0.7);
    println!("✓ Top edge constrained: ymin={:.3}, ymax=0.7 (gap={:.3})", 
             zone.bbox.ymin, zone.bbox.ymax - zone.bbox.ymin);
}

#[test]
fn test_right_edge_decrease_with_limit() {
    // Left arrow: Move right edge LEFT (decrease xmax, but stop near xmin)
    let mut app = App::new();
    app.zone_draft = Some(create_test_zone());
    
    // Try to decrease xmax way below xmin
    app.adjust_zone_bbox(0.0, 0.0, -0.5, 0.0); // Try to set xmax=0.2
    
    let zone = app.zone_draft.as_ref().unwrap();
    // Should be clamped to xmin + MIN_ZONE_SIZE
    let expected_xmax = 0.3 + MIN_ZONE_SIZE;
    assert!((zone.bbox.xmax - expected_xmax).abs() < 0.001);
    assert_eq!(zone.bbox.xmin, 0.3);
    println!("✓ Right edge constrained: xmin=0.3, xmax={:.3} (gap={:.3})", 
             zone.bbox.xmax, zone.bbox.xmax - zone.bbox.xmin);
}

#[test]
fn test_right_edge_increase() {
    // Right arrow: Move right edge RIGHT (increase xmax)
    let mut app = App::new();
    app.zone_draft = Some(create_test_zone());
    
    // Increase xmax by 0.05 (moving right edge right)
    app.adjust_zone_bbox(0.0, 0.0, 0.05, 0.0);
    
    let zone = app.zone_draft.as_ref().unwrap();
    assert_eq!(zone.bbox.xmin, 0.3);
    assert_eq!(zone.bbox.xmax, 0.75);
    println!("✓ Right edge increased: xmin=0.3, xmax=0.75");
}

#[test]
fn test_bottom_edge_decrease_with_limit() {
    // Up arrow: Move bottom edge UP (decrease ymax, but stop near ymin)
    let mut app = App::new();
    app.zone_draft = Some(create_test_zone());
    
    // Try to decrease ymax way below ymin
    app.adjust_zone_bbox(0.0, 0.0, 0.0, -0.5); // Try to set ymax=0.2
    
    let zone = app.zone_draft.as_ref().unwrap();
    // Should be clamped to ymin + MIN_ZONE_SIZE
    let expected_ymax = 0.3 + MIN_ZONE_SIZE;
    assert!((zone.bbox.ymax - expected_ymax).abs() < 0.001);
    assert_eq!(zone.bbox.ymin, 0.3);
    println!("✓ Bottom edge constrained: ymin=0.3, ymax={:.3} (gap={:.3})", 
             zone.bbox.ymax, zone.bbox.ymax - zone.bbox.ymin);
}

#[test]
fn test_bottom_edge_increase() {
    // Down arrow: Move bottom edge DOWN (increase ymax)
    let mut app = App::new();
    app.zone_draft = Some(create_test_zone());
    
    // Increase ymax by 0.05 (moving bottom edge down)
    app.adjust_zone_bbox(0.0, 0.0, 0.0, 0.05);
    
    let zone = app.zone_draft.as_ref().unwrap();
    assert_eq!(zone.bbox.ymin, 0.3);
    assert_eq!(zone.bbox.ymax, 0.75);
    println!("✓ Bottom edge increased: ymin=0.3, ymax=0.75");
}

#[test]
fn test_minimum_zone_size_maintained() {
    // Test that zone can never shrink below MIN_ZONE_SIZE
    let mut app = App::new();
    let mut small_zone = RoiZone::new_with_bbox(
        "Tiny Zone".to_string(),
        RoiBBox {
            xmin: 0.5,
            ymin: 0.5,
            xmax: 0.52,
            ymax: 0.52,
        },
    );
    small_zone.validate_and_clamp();
    app.zone_draft = Some(small_zone);
    
    // Try to shrink it further
    app.adjust_zone_bbox(0.01, 0.01, -0.01, -0.01);
    
    let zone = app.zone_draft.as_ref().unwrap();
    let width = zone.bbox.xmax - zone.bbox.xmin;
    let height = zone.bbox.ymax - zone.bbox.ymin;
    
    assert!(width >= MIN_ZONE_SIZE);
    assert!(height >= MIN_ZONE_SIZE);
    println!("✓ Minimum size maintained: width={:.3}, height={:.3}", width, height);
}

#[test]
fn test_boundary_clamping() {
    // Test that zones clamp to [0.0, 1.0] boundaries
    let mut app = App::new();
    app.zone_draft = Some(RoiZone::new_with_bbox(
        "Edge Zone".to_string(),
        RoiBBox {
            xmin: 0.05,
            ymin: 0.05,
            xmax: 0.95,
            ymax: 0.95,
        },
    ));
    
    // Try to move beyond boundaries
    app.adjust_zone_bbox(-0.2, -0.2, 0.2, 0.2);
    
    let zone = app.zone_draft.as_ref().unwrap();
    assert!(zone.bbox.xmin >= 0.0);
    assert!(zone.bbox.ymin >= 0.0);
    assert!(zone.bbox.xmax <= 1.0);
    assert!(zone.bbox.ymax <= 1.0);
    println!("✓ Boundaries respected: ({:.2},{:.2}) to ({:.2},{:.2})",
             zone.bbox.xmin, zone.bbox.ymin, zone.bbox.xmax, zone.bbox.ymax);
}

fn main() {
    println!("\n=== Zone Adjustment Boundary Fix Tests ===\n");
    
    test_left_edge_decrease();
    test_left_edge_increase_with_limit();
    test_top_edge_decrease();
    test_top_edge_increase_with_limit();
    test_right_edge_decrease_with_limit();
    test_right_edge_increase();
    test_bottom_edge_decrease_with_limit();
    test_bottom_edge_increase();
    test_minimum_zone_size_maintained();
    test_boundary_clamping();
    
    println!("\n=== All Tests Passed! ✓ ===\n");
    println!("Zone adjustment fix working correctly:");
    println!("  ✓ Left edge can move left/right within constraints");
    println!("  ✓ Top edge can move up/down within constraints");
    println!("  ✓ Right edge can move left/right within constraints");
    println!("  ✓ Bottom edge can move up/down within constraints");
    println!("  ✓ Minimum zone size of {:.1}% enforced", MIN_ZONE_SIZE * 100.0);
    println!("  ✓ Boundaries [0.0, 1.0] respected");
    println!("\nZone adjustment is now fully functional!");
}
