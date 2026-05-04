use std::{future::Future, num::NonZeroU8, pin::Pin, time::Duration};

use brother_ql::{media::Media, printjob::PrintJobBuilder};
use image::{DynamicImage, ImageBuffer, RgbImage};
use qrcode::{Color, QrCode};
use tokio::{io::AsyncWriteExt, net::TcpStream, time::timeout};
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    types::{LabelPrinterDriver, LabelPrinterMedia},
    AppState,
};

#[derive(Debug, Clone)]
pub struct LabelJob {
    pub batch_id: Uuid,
    pub batch_url: String,
    pub product_name: String,
    pub brand: Option<String>,
    pub location_name: String,
    pub quantity: String,
    pub unit: String,
    pub expires_on: Option<String>,
    pub opened_on: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RenderedLabel {
    pub driver: LabelPrinterDriver,
    pub media: LabelPrinterMedia,
    image: DynamicImage,
    pub width_px: usize,
    pub height_px: usize,
}

#[derive(Debug, Clone)]
pub struct PrintReceipt {
    pub bytes_sent: usize,
}

pub trait LabelRenderer {
    fn render(&self, job: &LabelJob, media: LabelPrinterMedia) -> ApiResult<RenderedLabel>;
}

pub trait LabelPrinter {
    fn print<'a>(
        &'a self,
        label: &'a RenderedLabel,
        copies: u8,
    ) -> Pin<Box<dyn Future<Output = ApiResult<PrintReceipt>> + Send + 'a>>;
}

pub async fn build_label_job(
    state: &AppState,
    household_id: Uuid,
    batch_id: Uuid,
) -> ApiResult<LabelJob> {
    let row = qm_db::stock::get_with_product(&state.db, household_id, batch_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(LabelJob {
        batch_id,
        batch_url: public_batch_url(state, batch_id)?,
        product_name: row.product.name,
        brand: row.product.brand,
        location_name: row.location_name,
        quantity: row.batch.quantity,
        unit: row.batch.unit,
        expires_on: row.batch.expires_on,
        opened_on: row.batch.opened_on,
        note: row.batch.note,
    })
}

pub fn public_batch_url(state: &AppState, batch_id: Uuid) -> ApiResult<String> {
    let base = state.config.public_base_url.as_deref().ok_or_else(|| {
        ApiError::BadRequest("QM_PUBLIC_BASE_URL is required before printing QR labels".into())
    })?;
    Ok(format!("{}/batches/{batch_id}", base.trim_end_matches('/')))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BrotherQlRenderer;

impl LabelRenderer for BrotherQlRenderer {
    fn render(&self, job: &LabelJob, media: LabelPrinterMedia) -> ApiResult<RenderedLabel> {
        let spec = BrotherMediaSpec::for_media(media);
        let mut bitmap = Bitmap::new(spec.width_px, spec.height_px);
        bitmap.draw_rect(0, 0, spec.width_px, spec.height_px, Pixel::White);
        bitmap.draw_rect(0, 0, spec.width_px, spec.height_px, Pixel::Black);
        bitmap.draw_rect(
            2,
            2,
            spec.width_px.saturating_sub(4),
            spec.height_px.saturating_sub(4),
            Pixel::White,
        );
        if spec.supports_red {
            bitmap.draw_rect(2, 2, spec.width_px.saturating_sub(4), 14, Pixel::Red);
        }

        let qr = QrCode::new(job.batch_url.as_bytes())
            .map_err(|err| ApiError::Internal(anyhow::anyhow!("encoding batch QR: {err}")))?;
        let qr_modules = qr.width();
        let qr_px = spec.qr_px.min(spec.height_px.saturating_sub(16));
        let module_px = (qr_px / (qr_modules + 8)).max(2);
        let actual_qr_px = module_px * (qr_modules + 8);
        let qr_x = 10;
        let qr_y = (spec.height_px.saturating_sub(actual_qr_px)) / 2;
        bitmap.draw_qr(&qr, qr_x, qr_y, module_px);

        let text_x = qr_x + actual_qr_px + 14;
        let mut y = if spec.supports_red { 22 } else { 12 };
        bitmap.draw_text(text_x, y, &truncate(&job.product_name, 26), 3);
        y += 28;
        if let Some(brand) = job.brand.as_deref().filter(|s| !s.trim().is_empty()) {
            bitmap.draw_text(text_x, y, &truncate(brand, 30), 2);
            y += 20;
        }
        bitmap.draw_text(
            text_x,
            y,
            &format!("{} {} - {}", job.quantity, job.unit, job.location_name),
            2,
        );
        y += 20;
        if let Some(expires_on) = &job.expires_on {
            bitmap.draw_text(text_x, y, &format!("EXP {expires_on}"), 2);
            y += 20;
        }
        if let Some(opened_on) = &job.opened_on {
            bitmap.draw_text(text_x, y, &format!("OPEN {opened_on}"), 2);
            y += 20;
        }
        if let Some(note) = job.note.as_deref().filter(|s| !s.trim().is_empty()) {
            bitmap.draw_text(text_x, y, &truncate(note, 34), 1);
        }
        bitmap.draw_text(
            text_x,
            spec.height_px.saturating_sub(16),
            &format!("BATCH {}", short_id(job.batch_id)),
            1,
        );

        Ok(RenderedLabel {
            driver: LabelPrinterDriver::BrotherQlRaster,
            media,
            image: bitmap.into_image(),
            width_px: spec.width_px,
            height_px: spec.height_px,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BrotherQlRasterPrinter {
    pub address: String,
    pub port: u16,
}

impl LabelPrinter for BrotherQlRasterPrinter {
    fn print<'a>(
        &'a self,
        label: &'a RenderedLabel,
        copies: u8,
    ) -> Pin<Box<dyn Future<Output = ApiResult<PrintReceipt>> + Send + 'a>> {
        Box::pin(async move {
            let bytes = compile_brother_ql_job(label, copies)?;
            let mut stream = timeout(
                Duration::from_secs(5),
                TcpStream::connect((self.address.as_str(), self.port)),
            )
            .await
            .map_err(|_| ApiError::BadGateway)?
            .map_err(|err| {
                tracing::warn!(?err, "failed to connect to label printer");
                ApiError::BadGateway
            })?;
            stream.write_all(&bytes).await.map_err(|err| {
                tracing::warn!(?err, "failed to send label bytes");
                ApiError::BadGateway
            })?;
            stream.shutdown().await.map_err(|err| {
                tracing::warn!(?err, "failed to close label printer socket");
                ApiError::BadGateway
            })?;
            Ok(PrintReceipt {
                bytes_sent: bytes.len(),
            })
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct BrotherMediaSpec {
    width_px: usize,
    height_px: usize,
    media: Media,
    qr_px: usize,
    supports_red: bool,
}

impl BrotherMediaSpec {
    fn for_media(media: LabelPrinterMedia) -> Self {
        match media {
            LabelPrinterMedia::Dk62Continuous => Self {
                width_px: Media::C62.width_dots() as usize,
                height_px: 360,
                media: Media::C62,
                qr_px: 300,
                supports_red: false,
            },
            LabelPrinterMedia::Dk62RedBlackContinuous => Self {
                width_px: Media::C62R.width_dots() as usize,
                height_px: 360,
                media: Media::C62R,
                qr_px: 300,
                supports_red: true,
            },
            LabelPrinterMedia::Dk29x90 => Self {
                width_px: Media::D29x90.width_dots() as usize,
                height_px: Media::D29x90.length_dots().unwrap() as usize,
                media: Media::D29x90,
                qr_px: 240,
                supports_red: false,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pixel {
    White,
    Black,
    Red,
}

#[derive(Debug, Clone)]
struct Bitmap {
    width: usize,
    height: usize,
    pixels: Vec<Pixel>,
}

impl Bitmap {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![Pixel::White; width * height],
        }
    }

    fn set(&mut self, x: usize, y: usize, pixel: Pixel) {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x] = pixel;
        }
    }

    fn get(&self, x: usize, y: usize) -> Pixel {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x]
        } else {
            Pixel::White
        }
    }

    fn draw_rect(&mut self, x: usize, y: usize, width: usize, height: usize, pixel: Pixel) {
        for yy in y..y.saturating_add(height).min(self.height) {
            for xx in x..x.saturating_add(width).min(self.width) {
                self.set(xx, yy, pixel);
            }
        }
    }

    fn draw_qr(&mut self, qr: &QrCode, x: usize, y: usize, module_px: usize) {
        let modules = qr.width();
        self.draw_rect(
            x,
            y,
            module_px * (modules + 8),
            module_px * (modules + 8),
            Pixel::White,
        );
        for my in 0..modules {
            for mx in 0..modules {
                if qr[(mx, my)] == Color::Dark {
                    self.draw_rect(
                        x + module_px * (mx + 4),
                        y + module_px * (my + 4),
                        module_px,
                        module_px,
                        Pixel::Black,
                    );
                }
            }
        }
    }

    fn draw_text(&mut self, x: usize, y: usize, text: &str, scale: usize) {
        let mut cursor = x;
        for ch in text.chars().flat_map(char::to_uppercase) {
            if cursor + 6 * scale >= self.width {
                break;
            }
            draw_char(self, cursor, y, ch, scale.max(1));
            cursor += 6 * scale.max(1);
        }
    }

    fn into_image(self) -> DynamicImage {
        let image: RgbImage =
            ImageBuffer::from_fn(self.width as u32, self.height as u32, |x, y| {
                match self.get(x as usize, y as usize) {
                    Pixel::White => image::Rgb([255, 255, 255]),
                    Pixel::Black => image::Rgb([0, 0, 0]),
                    Pixel::Red => image::Rgb([220, 0, 0]),
                }
            });
        DynamicImage::ImageRgb8(image)
    }
}

fn compile_brother_ql_job(label: &RenderedLabel, copies: u8) -> ApiResult<Vec<u8>> {
    let media = BrotherMediaSpec::for_media(label.media).media;
    let copies = NonZeroU8::new(copies.max(1)).ok_or_else(|| {
        ApiError::Internal(anyhow::anyhow!("label print copies must be non-zero"))
    })?;
    let job = PrintJobBuilder::new(media)
        .copies(copies)
        .add_label(label.image.clone())
        .build()
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("building Brother QL job: {err}")))?;
    Ok(job.compile())
}

fn draw_char(bitmap: &mut Bitmap, x: usize, y: usize, ch: char, scale: usize) {
    let glyph = glyph(ch);
    for (row, bits) in glyph.iter().enumerate() {
        for col in 0..5 {
            if bits & (1 << (4 - col)) != 0 {
                bitmap.draw_rect(x + col * scale, y + row * scale, scale, scale, Pixel::Black);
            }
        }
    }
}

fn glyph(ch: char) -> [u8; 7] {
    match ch {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1f],
        'J' => [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0e],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        '6' => [0x0e, 0x10, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x01, 0x0e],
        '-' => [0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c],
        ':' => [0x00, 0x0c, 0x0c, 0x00, 0x0c, 0x0c, 0x00],
        ' ' => [0; 7],
        _ => [0x1f, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn short_id(id: Uuid) -> String {
    id.to_string()
        .chars()
        .rev()
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brother_renderer_outputs_deterministic_raster_bytes() {
        let job = LabelJob {
            batch_id: Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap(),
            batch_url:
                "https://quartermaster.example.com/batches/33333333-3333-3333-3333-333333333333"
                    .into(),
            product_name: "Flour".into(),
            brand: Some("Acme".into()),
            location_name: "Pantry".into(),
            quantity: "500".into(),
            unit: "g".into(),
            expires_on: Some("2026-06-01".into()),
            opened_on: None,
            note: Some("bag".into()),
        };

        let rendered = BrotherQlRenderer
            .render(&job, LabelPrinterMedia::Dk62Continuous)
            .unwrap();
        let bytes = compile_brother_ql_job(&rendered, 1).unwrap();
        assert_eq!(rendered.width_px, 696);
        assert_eq!(rendered.height_px, 360);
        assert!(bytes.starts_with(&[0x00; 10]));
        assert_eq!(bytes.last(), Some(&0x1a));
        assert_eq!(bytes.len(), 33923);
    }

    #[test]
    fn brother_renderer_uses_two_color_mode_for_red_black_continuous_media() {
        let job = LabelJob {
            batch_id: Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap(),
            batch_url:
                "https://quartermaster.example.com/batches/33333333-3333-3333-3333-333333333333"
                    .into(),
            product_name: "Flour".into(),
            brand: Some("Acme".into()),
            location_name: "Pantry".into(),
            quantity: "500".into(),
            unit: "g".into(),
            expires_on: Some("2026-06-01".into()),
            opened_on: None,
            note: Some("bag".into()),
        };

        let rendered = BrotherQlRenderer
            .render(&job, LabelPrinterMedia::Dk62RedBlackContinuous)
            .unwrap();
        let bytes = compile_brother_ql_job(&rendered, 1).unwrap();
        let rgb = rendered.image.to_rgb8();

        assert_eq!(rendered.width_px, 696);
        assert_eq!(rendered.height_px, 360);
        assert_eq!(*rgb.get_pixel(3, 3), image::Rgb([220, 0, 0]));
        assert!(bytes
            .windows(4)
            .any(|chunk| chunk == [0x1b, 0x69, 0x4b, 0x01]));
        assert_eq!(bytes.last(), Some(&0x1a));
    }

    #[test]
    fn brother_renderer_uses_brother_ql_media_dimensions() {
        let job = LabelJob {
            batch_id: Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap(),
            batch_url:
                "https://quartermaster.example.com/batches/33333333-3333-3333-3333-333333333333"
                    .into(),
            product_name: "Flour".into(),
            brand: None,
            location_name: "Pantry".into(),
            quantity: "500".into(),
            unit: "g".into(),
            expires_on: None,
            opened_on: None,
            note: None,
        };

        let rendered = BrotherQlRenderer
            .render(&job, LabelPrinterMedia::Dk29x90)
            .unwrap();
        let bytes = compile_brother_ql_job(&rendered, 2).unwrap();

        assert_eq!(rendered.width_px, 306);
        assert_eq!(rendered.height_px, 944);
        assert!(bytes.starts_with(&[0x00; 10]));
        assert_eq!(bytes.last(), Some(&0x1a));
    }
}
