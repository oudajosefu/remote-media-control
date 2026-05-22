use std::path::{Path, PathBuf};

// Generates icon-active.png and icon-inactive.png in server/assets/ at build time.
// Also emits a Windows .ico from the same base artwork so installed app surfaces
// can reuse the tray icon branding.

fn main() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    std::fs::create_dir_all(&out).unwrap();

    let inactive = render_icon(16, false);
    write_png(&out.join("icon-inactive.png"), 16, 16, &inactive);

    let active = render_icon(16, true);
    write_png(&out.join("icon-active.png"), 16, 16, &active);

    let app_icon = out.join("app-icon.ico");
    write_ico(&app_icon, &[16, 32, 48, 64, 128, 256]);
    embed_windows_resources(&app_icon);

    println!("cargo:rerun-if-changed=build.rs");
}

fn render_icon(size: u32, active: bool) -> Vec<u8> {
    let width = size as usize;
    let height = size as usize;
    let scale = size as f32 / 16.0;
    let mut pixels = vec![0u8; width * height * 4];

    // Draw play triangle (pointing right).
    let fg = [230u8, 230, 230, 255];
    let base_x = 4.0 * scale;
    let tip_x = 12.0 * scale;
    let center_y = 7.5 * scale;
    let max_half_height = 5.0 * scale;
    for y in 0..height {
        for x in 0..width {
            let fx = x as f32;
            let fy = y as f32;
            if fx >= base_x && fx <= tip_x {
                let t = (tip_x - fx) / (8.0 * scale);
                let half_height = max_half_height * t;
                if fy >= center_y - half_height && fy <= center_y + half_height {
                    let idx = (y * width + x) * 4;
                    pixels[idx..idx + 4].copy_from_slice(&fg);
                }
            }
        }
    }

    if active {
        let cx = 12.5 * scale;
        let cy = 12.5 * scale;
        let dark_green = [18u8, 140, 65, 255];
        let bright_green = [34u8, 197, 94, 255];
        let outer_radius = 3.2 * scale;
        let inner_radius = 2.5 * scale;

        for y in 0..height {
            for x in 0..width {
                let fx = x as f32 + 0.5;
                let fy = y as f32 + 0.5;
                let dist = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                let idx = (y * width + x) * 4;
                if dist <= outer_radius {
                    pixels[idx..idx + 4].copy_from_slice(&dark_green);
                }
                if dist <= inner_radius {
                    pixels[idx..idx + 4].copy_from_slice(&bright_green);
                }
            }
        }
    }

    pixels
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) {
    let bytes = encode_png(width, height, rgba);
    std::fs::write(path, bytes).unwrap();
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(std::io::Cursor::new(&mut buf), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(rgba).unwrap();
    }
    buf
}

fn write_ico(path: &Path, sizes: &[u32]) {
    let images: Vec<(u32, Vec<u8>)> = sizes
        .iter()
        .copied()
        .map(|size| {
            let rgba = render_icon(size, false);
            (size, encode_png(size, size, &rgba))
        })
        .collect();

    let directory_size = 6 + (16 * images.len());
    let mut offset = directory_size as u32;
    let mut icon = Vec::new();
    let mut payload = Vec::new();

    icon.extend_from_slice(&0u16.to_le_bytes());
    icon.extend_from_slice(&1u16.to_le_bytes());
    icon.extend_from_slice(&(images.len() as u16).to_le_bytes());

    for (size, png) in &images {
        let dimension = if *size >= 256 { 0 } else { *size as u8 };
        icon.push(dimension);
        icon.push(dimension);
        icon.push(0);
        icon.push(0);
        icon.extend_from_slice(&1u16.to_le_bytes());
        icon.extend_from_slice(&32u16.to_le_bytes());
        icon.extend_from_slice(&(png.len() as u32).to_le_bytes());
        icon.extend_from_slice(&offset.to_le_bytes());
        payload.extend_from_slice(png);
        offset += png.len() as u32;
    }

    icon.extend_from_slice(&payload);
    std::fs::write(path, icon).unwrap();
}

#[cfg(target_os = "windows")]
fn embed_windows_resources(icon_path: &Path) {
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(icon_path.to_str().unwrap());
    resource.compile().unwrap();
}

#[cfg(not(target_os = "windows"))]
fn embed_windows_resources(_icon_path: &Path) {}
