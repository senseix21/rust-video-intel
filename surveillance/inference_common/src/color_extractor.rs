//! Color extraction from image regions for object attribute detection.

use image::{DynamicImage, GenericImageView};

/// Extract dominant color from a bounding box region
pub fn extract_dominant_color(
    image: &DynamicImage,
    xmin: f32,
    ymin: f32,
    xmax: f32,
    ymax: f32,
) -> Option<(u8, u8, u8)> {
    let (img_width, img_height) = image.dimensions();
    
    // Clamp coordinates to image bounds
    let x1 = xmin.max(0.0).min(img_width as f32) as u32;
    let y1 = ymin.max(0.0).min(img_height as f32) as u32;
    let x2 = xmax.max(0.0).min(img_width as f32) as u32;
    let y2 = ymax.max(0.0).min(img_height as f32) as u32;
    
    if x2 <= x1 || y2 <= y1 {
        return None;
    }
    
    // Sample colors from the region (focus on center area to avoid edge artifacts)
    let margin_x = ((x2 - x1) as f32 * 0.2) as u32;
    let margin_y = ((y2 - y1) as f32 * 0.2) as u32;
    
    let sample_x1 = (x1 + margin_x).min(x2);
    let sample_y1 = (y1 + margin_y).min(y2);
    let sample_x2 = (x2 - margin_x).max(x1);
    let sample_y2 = (y2 - margin_y).max(y1);
    
    if sample_x2 <= sample_x1 || sample_y2 <= sample_y1 {
        // Fallback to full bbox if margins make it invalid
        return extract_simple_average(image, x1, y1, x2, y2);
    }
    
    extract_simple_average(image, sample_x1, sample_y1, sample_x2, sample_y2)
}

/// Extract simple average color from a region
fn extract_simple_average(
    image: &DynamicImage,
    x1: u32,
    y1: u32,
    x2: u32,
    y2: u32,
) -> Option<(u8, u8, u8)> {
    let mut r_sum: u64 = 0;
    let mut g_sum: u64 = 0;
    let mut b_sum: u64 = 0;
    let mut count: u64 = 0;
    
    // Sample every few pixels for performance
    let step = ((x2 - x1).max(y2 - y1) / 20).max(1);
    
    for y in (y1..y2).step_by(step as usize) {
        for x in (x1..x2).step_by(step as usize) {
            let pixel = image.get_pixel(x, y);
            r_sum += pixel[0] as u64;
            g_sum += pixel[1] as u64;
            b_sum += pixel[2] as u64;
            count += 1;
        }
    }
    
    if count == 0 {
        return None;
    }
    
    Some((
        (r_sum / count) as u8,
        (g_sum / count) as u8,
        (b_sum / count) as u8,
    ))
}

/// Extract dominant color using histogram-based approach (more accurate but slower)
#[allow(dead_code)]
pub fn extract_histogram_color(
    image: &DynamicImage,
    xmin: f32,
    ymin: f32,
    xmax: f32,
    ymax: f32,
) -> Option<(u8, u8, u8)> {
    let (img_width, img_height) = image.dimensions();
    
    let x1 = xmin.max(0.0).min(img_width as f32) as u32;
    let y1 = ymin.max(0.0).min(img_height as f32) as u32;
    let x2 = xmax.max(0.0).min(img_width as f32) as u32;
    let y2 = ymax.max(0.0).min(img_height as f32) as u32;
    
    if x2 <= x1 || y2 <= y1 {
        return None;
    }
    
    // Quantize colors to reduce histogram size (reduce to 4 bits per channel = 16 values)
    const BINS: usize = 16;
    let mut histogram = vec![0u32; BINS * BINS * BINS];
    
    let step = ((x2 - x1).max(y2 - y1) / 20).max(1);
    
    for y in (y1..y2).step_by(step as usize) {
        for x in (x1..x2).step_by(step as usize) {
            let pixel = image.get_pixel(x, y);
            let r_bin = (pixel[0] as usize * BINS / 256).min(BINS - 1);
            let g_bin = (pixel[1] as usize * BINS / 256).min(BINS - 1);
            let b_bin = (pixel[2] as usize * BINS / 256).min(BINS - 1);
            
            let idx = r_bin * BINS * BINS + g_bin * BINS + b_bin;
            histogram[idx] += 1;
        }
    }
    
    // Find most common color
    let (max_idx, _max_count) = histogram
        .iter()
        .enumerate()
        .max_by_key(|(_, &count)| count)?;
    
    let r_bin = max_idx / (BINS * BINS);
    let g_bin = (max_idx / BINS) % BINS;
    let b_bin = max_idx % BINS;
    
    Some((
        (r_bin * 256 / BINS) as u8,
        (g_bin * 256 / BINS) as u8,
        (b_bin * 256 / BINS) as u8,
    ))
}
