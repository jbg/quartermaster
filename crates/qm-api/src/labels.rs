use std::{future::Future, net::SocketAddr, pin::Pin, time::Duration};

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
    pub bytes: Vec<u8>,
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
        bitmap.draw_rect(0, 0, spec.width_px, spec.height_px, false);
        bitmap.draw_rect(0, 0, spec.width_px, spec.height_px, true);
        bitmap.draw_rect(
            2,
            2,
            spec.width_px.saturating_sub(4),
            spec.height_px.saturating_sub(4),
            false,
        );

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
        let mut y = 12;
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

        let bytes = brother_raster_bytes(&bitmap, spec);
        Ok(RenderedLabel {
            driver: LabelPrinterDriver::BrotherQlRaster,
            media,
            bytes,
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
            let addr: SocketAddr =
                format!("{}:{}", self.address, self.port)
                    .parse()
                    .map_err(|_| {
                        ApiError::BadRequest("printer address must be host:port compatible".into())
                    })?;
            let mut stream = timeout(Duration::from_secs(5), TcpStream::connect(addr))
                .await
                .map_err(|_| ApiError::BadGateway)?
                .map_err(|err| {
                    tracing::warn!(?err, "failed to connect to label printer");
                    ApiError::BadGateway
                })?;
            let mut sent = 0usize;
            for _ in 0..copies.max(1) {
                stream.write_all(&label.bytes).await.map_err(|err| {
                    tracing::warn!(?err, "failed to send label bytes");
                    ApiError::BadGateway
                })?;
                sent += label.bytes.len();
            }
            stream.shutdown().await.map_err(|err| {
                tracing::warn!(?err, "failed to close label printer socket");
                ApiError::BadGateway
            })?;
            Ok(PrintReceipt { bytes_sent: sent })
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct BrotherMediaSpec {
    width_px: usize,
    height_px: usize,
    media_type: u8,
    media_width_mm: u8,
    media_length_mm: u8,
    qr_px: usize,
}

impl BrotherMediaSpec {
    fn for_media(media: LabelPrinterMedia) -> Self {
        match media {
            LabelPrinterMedia::Dk62Continuous => Self {
                width_px: 696,
                height_px: 360,
                media_type: 0x0a,
                media_width_mm: 62,
                media_length_mm: 0,
                qr_px: 300,
            },
            LabelPrinterMedia::Dk29x90 => Self {
                width_px: 306,
                height_px: 991,
                media_type: 0x0b,
                media_width_mm: 29,
                media_length_mm: 90,
                qr_px: 240,
            },
        }
    }
}

#[derive(Debug, Clone)]
struct Bitmap {
    width: usize,
    height: usize,
    pixels: Vec<bool>,
}

impl Bitmap {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![false; width * height],
        }
    }

    fn set(&mut self, x: usize, y: usize, black: bool) {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x] = black;
        }
    }

    fn get(&self, x: usize, y: usize) -> bool {
        x < self.width && y < self.height && self.pixels[y * self.width + x]
    }

    fn draw_rect(&mut self, x: usize, y: usize, width: usize, height: usize, black: bool) {
        for yy in y..y.saturating_add(height).min(self.height) {
            for xx in x..x.saturating_add(width).min(self.width) {
                self.set(xx, yy, black);
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
            false,
        );
        for my in 0..modules {
            for mx in 0..modules {
                if qr[(mx, my)] == Color::Dark {
                    self.draw_rect(
                        x + module_px * (mx + 4),
                        y + module_px * (my + 4),
                        module_px,
                        module_px,
                        true,
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
}

fn brother_raster_bytes(bitmap: &Bitmap, spec: BrotherMediaSpec) -> Vec<u8> {
    let bytes_per_row = (bitmap.width + 7) / 8;
    let mut out = Vec::with_capacity(bitmap.height * (bytes_per_row + 3) + 64);
    out.extend_from_slice(&[0x00; 10]);
    out.extend_from_slice(b"\x1B\x40");
    out.extend_from_slice(b"\x1B\x69\x61\x01");
    out.extend_from_slice(&[
        0x1b,
        0x69,
        0x7a,
        0xe7,
        spec.media_type,
        spec.media_width_mm,
        spec.media_length_mm,
        (bitmap.height & 0xff) as u8,
        ((bitmap.height >> 8) & 0xff) as u8,
        ((bitmap.height >> 16) & 0xff) as u8,
        ((bitmap.height >> 24) & 0xff) as u8,
        0,
        0,
    ]);
    out.extend_from_slice(b"\x1B\x69\x4D\x40");
    out.extend_from_slice(b"\x1B\x69\x64\x00\x00");
    for y in 0..bitmap.height {
        out.push(b'g');
        out.push((bytes_per_row & 0xff) as u8);
        out.push(((bytes_per_row >> 8) & 0xff) as u8);
        for byte_index in 0..bytes_per_row {
            let mut byte = 0u8;
            for bit in 0..8 {
                let x = byte_index * 8 + bit;
                if bitmap.get(x, y) {
                    byte |= 0x80 >> bit;
                }
            }
            out.push(byte);
        }
    }
    out.push(0x1a);
    out
}

fn draw_char(bitmap: &mut Bitmap, x: usize, y: usize, ch: char, scale: usize) {
    let glyph = glyph(ch);
    for (row, bits) in glyph.iter().enumerate() {
        for col in 0..5 {
            if bits & (1 << (4 - col)) != 0 {
                bitmap.draw_rect(x + col * scale, y + row * scale, scale, scale, true);
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
        assert_eq!(rendered.width_px, 696);
        assert_eq!(rendered.height_px, 360);
        assert!(rendered.bytes.starts_with(&[0x00; 10]));
        assert_eq!(rendered.bytes.last(), Some(&0x1a));
        assert_eq!(rendered.bytes.len(), 32439);
    }
}
