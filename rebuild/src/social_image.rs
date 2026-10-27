//! Self-contained raster link cards: no outbound request includes task content.
use crate::core::Error;
use rusttype::{Font, FontCollection, Scale, point};
fn font(bold: bool) -> Result<Font<'static>, Error> {
    FontCollection::from_bytes(if bold {
        include_bytes!("../assets/fonts/ReportSans-Bold.ttf").as_slice()
    } else {
        include_bytes!("../assets/fonts/ReportSans.ttf").as_slice()
    })
    .and_then(|c| c.into_font())
    .map_err(|_| Error::Invalid("Could not load preview font".into()))
}
fn draw(pixels: &mut [u8], font: &Font, text: &str, size: f32, x: f32, y: f32, color: [u8; 3]) {
    for glyph in font.layout(text, Scale::uniform(size), point(x, y)) {
        if let Some(bounds) = glyph.pixel_bounding_box() {
            glyph.draw(|gx, gy, alpha| {
                let x = bounds.min.x + gx as i32;
                let y = bounds.min.y + gy as i32;
                if (0..1200).contains(&x) && (0..630).contains(&y) {
                    let i = ((y * 1200 + x) * 3) as usize;
                    for c in 0..3 {
                        pixels[i + c] =
                            (pixels[i + c] as f32 * (1. - alpha) + color[c] as f32 * alpha) as u8;
                    }
                }
            });
        }
    }
}
fn width(font: &Font, text: &str, size: f32) -> f32 {
    font.layout(text, Scale::uniform(size), point(0., 0.))
        .last()
        .map(|g| g.position().x + g.unpositioned().h_metrics().advance_width)
        .unwrap_or(0.)
}
fn lines(font: &Font, text: &str, size: f32, max: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        for part in word.chars().collect::<Vec<_>>().chunks(45) {
            let part: String = part.iter().collect();
            let next = if line.is_empty() {
                part.clone()
            } else {
                format!("{line} {part}")
            };
            if width(font, &next, size) > 1000. && !line.is_empty() {
                result.push(line);
                line = part;
            } else {
                line = next;
            }
            if result.len() >= max {
                if let Some(last) = result.last_mut() {
                    last.push('…');
                }
                return result;
            }
        }
    }
    if !line.is_empty() {
        result.push(line);
    }
    result
}
pub fn render(
    title: &str,
    description: &str,
    kind: &str,
    complete: bool,
) -> Result<Vec<u8>, Error> {
    let mut pixels = vec![0; 1200 * 630 * 3];
    for y in 0..630usize {
        for x in 0..1200usize {
            let glow = ((x as f32 / 1200.) * (1. - y as f32 / 700.) * 12.) as u8;
            let i = (y * 1200 + x) * 3;
            pixels[i] = 5 + glow;
            pixels[i + 1] = 9 + glow;
            pixels[i + 2] = 18 + glow;
        }
    }
    let bold = font(true)?;
    let regular = font(false)?;
    draw(&mut pixels, &bold, "todoer", 32., 76., 89., [221, 230, 240]);
    draw(
        &mut pixels,
        &regular,
        "pro",
        17.,
        186.,
        74.,
        [139, 132, 255],
    );
    draw(
        &mut pixels,
        &regular,
        &format!("{} · READ ONLY", kind.to_uppercase()),
        16.,
        76.,
        155.,
        [137, 153, 175],
    );
    let title_lines = lines(&bold, title, 54., 3);
    let mut y = 230.;
    for line in title_lines {
        draw(&mut pixels, &bold, &line, 54., 76., y, [231, 237, 245]);
        y += 70.;
    }
    let summary = description.replace(['\n', '\r'], " ");
    for line in lines(&regular, &summary, 23., 2) {
        draw(
            &mut pixels,
            &regular,
            &line,
            23.,
            78.,
            y + 20.,
            [152, 166, 188],
        );
        y += 34.;
    }
    for x in 76..1124 {
        let i = (540 * 1200 + x) * 3;
        pixels[i..i + 3].copy_from_slice(&[36, 48, 66]);
    }
    draw(
        &mut pixels,
        &regular,
        if kind == "task" {
            if complete {
                "Completed · Open task"
            } else {
                "One thing at a time · Open task"
            }
        } else {
            "A little focus goes a long way · todoer.pro"
        },
        17.,
        78.,
        585.,
        [137, 153, 175],
    );
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1200, 630);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| Error::Invalid("Could not render preview".into()))?;
        writer
            .write_image_data(&pixels)
            .map_err(|_| Error::Invalid("Could not render preview".into()))?;
    }
    Ok(bytes)
}
