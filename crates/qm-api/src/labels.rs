use std::{future::Future, num::NonZeroU8, pin::Pin, time::Duration};

use brother_ql::{media::Media, printjob::PrintJobBuilder};
use image::{DynamicImage, ImageBuffer, RgbImage};
use qrcode::{Color, EcLevel, QrCode};
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
    pub produced_on: Option<String>,
    pub expires_on: Option<String>,
    pub opened_on: Option<String>,
    pub note: Option<String>,
    pub include_quantity: bool,
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
        produced_on: row.batch.produced_on,
        expires_on: row.batch.expires_on,
        opened_on: row.batch.opened_on,
        note: row.batch.note,
        include_quantity: false,
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

        let qr = batch_qr(&job.batch_url)?;
        if spec.is_portrait() {
            render_portrait_label(&mut bitmap, &qr, job, &spec);
        } else {
            render_landscape_label(&mut bitmap, &qr, job, &spec);
        }

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

    fn is_portrait(&self) -> bool {
        self.height_px > self.width_px.saturating_mul(2)
    }
}

fn render_landscape_label(
    bitmap: &mut Bitmap,
    qr: &QrCode,
    job: &LabelJob,
    spec: &BrotherMediaSpec,
) {
    let (actual_qr_px, module_px) = qr_size(qr, spec.qr_px.min(spec.height_px.saturating_sub(24)));
    let qr_x = 10;
    let qr_y = (spec.height_px.saturating_sub(actual_qr_px)) / 2;
    bitmap.draw_branded_qr(qr, qr_x, qr_y, actual_qr_px, module_px);

    let text_x = qr_x + actual_qr_px + 18;
    let text_width = spec.width_px.saturating_sub(text_x + 8);
    let mut y = if spec.supports_red { 24 } else { 16 };
    y += draw_wrapped_text(bitmap, text_x, y, &job.product_name, text_width, 5, 2) + 12;
    if let Some(brand) = job.brand.as_deref().filter(|s| !s.trim().is_empty()) {
        bitmap.draw_text(text_x, y, &truncate_for_width(brand, text_width, 3), 3);
        y += 30;
    }
    if job.include_quantity {
        bitmap.draw_text(
            text_x,
            y,
            &truncate_for_width(&label_quantity(job), text_width, 3),
            3,
        );
        y += 34;
    }
    y += draw_primary_date(bitmap, text_x, y, text_width, job);
    y += draw_secondary_dates(bitmap, text_x, y + 4, text_width, job, 3);
    if let Some(note) = job.note.as_deref().filter(|s| !s.trim().is_empty()) {
        bitmap.draw_text(text_x, y + 6, &truncate_for_width(note, text_width, 2), 2);
    }
    bitmap.draw_text(
        text_x,
        spec.height_px.saturating_sub(16),
        &format!("BATCH {}", short_id(job.batch_id)),
        1,
    );
}

fn render_portrait_label(
    bitmap: &mut Bitmap,
    qr: &QrCode,
    job: &LabelJob,
    spec: &BrotherMediaSpec,
) {
    let (actual_qr_px, module_px) = qr_size(qr, spec.qr_px.min(spec.width_px.saturating_sub(28)));
    let qr_x = (spec.width_px.saturating_sub(actual_qr_px)) / 2;
    let qr_y = 18;
    bitmap.draw_branded_qr(qr, qr_x, qr_y, actual_qr_px, module_px);

    let text_x = 14;
    let text_width = spec.width_px.saturating_sub(text_x * 2);
    let mut y = qr_y + actual_qr_px + 28;
    y += draw_wrapped_text(bitmap, text_x, y, &job.product_name, text_width, 4, 2) + 10;
    if let Some(brand) = job.brand.as_deref().filter(|s| !s.trim().is_empty()) {
        bitmap.draw_text(text_x, y, &truncate_for_width(brand, text_width, 2), 2);
        y += 24;
    }
    if job.include_quantity {
        bitmap.draw_text(
            text_x,
            y,
            &truncate_for_width(&label_quantity(job), text_width, 2),
            2,
        );
        y += 32;
    }
    y += draw_primary_date(bitmap, text_x, y, text_width, job);
    y += draw_secondary_dates(bitmap, text_x, y + 8, text_width, job, 2);
    if let Some(note) = job.note.as_deref().filter(|s| !s.trim().is_empty()) {
        bitmap.draw_text(text_x, y + 10, &truncate_for_width(note, text_width, 2), 2);
    }
    bitmap.draw_text(
        text_x,
        spec.height_px.saturating_sub(18),
        &format!("BATCH {}", short_id(job.batch_id)),
        1,
    );
}

fn qr_size(qr: &QrCode, max_px: usize) -> (usize, usize) {
    let modules = qr.width();
    let module_px = (max_px / (modules + 8)).max(2);
    (module_px * (modules + 8), module_px)
}

fn batch_qr(batch_url: &str) -> ApiResult<QrCode> {
    QrCode::with_error_correction_level(batch_url.as_bytes(), EcLevel::H)
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("encoding batch QR: {err}")))
}

fn branded_qr_logo_px(actual_qr_px: usize, module_px: usize) -> usize {
    (actual_qr_px / 5).max(module_px * 7)
}

fn branded_qr_badge_px(actual_qr_px: usize, module_px: usize) -> usize {
    branded_qr_logo_px(actual_qr_px, module_px) + module_px * 4
}

fn label_quantity(job: &LabelJob) -> String {
    format!("{} {}", job.quantity.trim(), job.unit.trim())
        .trim()
        .to_owned()
}

fn draw_primary_date(
    bitmap: &mut Bitmap,
    x: usize,
    y: usize,
    width: usize,
    job: &LabelJob,
) -> usize {
    let Some((label, date)) = primary_date(job) else {
        return 0;
    };
    bitmap.draw_text(x, y, label, 2);
    let date_scale = if text_width(date, 5) <= width { 5 } else { 4 };
    bitmap.draw_text(x, y + 20, date, date_scale);
    20 + line_height(date_scale) + 8
}

fn draw_secondary_dates(
    bitmap: &mut Bitmap,
    x: usize,
    y: usize,
    width: usize,
    job: &LabelJob,
    scale: usize,
) -> usize {
    let mut lines = Vec::new();
    if job.expires_on.is_some() {
        if let Some(produced_on) = &job.produced_on {
            lines.push(format!("MADE {produced_on}"));
        }
    }
    if let Some(opened_on) = &job.opened_on {
        lines.push(format!("OPEN {opened_on}"));
    }
    let mut consumed = 0;
    for line in lines {
        bitmap.draw_text(
            x,
            y + consumed,
            &truncate_for_width(&line, width, scale),
            scale,
        );
        consumed += line_height(scale) + 6;
    }
    consumed
}

fn draw_wrapped_text(
    bitmap: &mut Bitmap,
    x: usize,
    y: usize,
    value: &str,
    width: usize,
    scale: usize,
    max_lines: usize,
) -> usize {
    let lines = wrap_for_width(value, width, scale, max_lines);
    let line_gap = (scale * 2).max(4);
    let mut consumed = 0;
    for line in lines {
        bitmap.draw_text(x, y + consumed, &line, scale);
        consumed += line_height(scale) + line_gap;
    }
    consumed.saturating_sub(line_gap)
}

fn wrap_for_width(value: &str, width: usize, scale: usize, max_lines: usize) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }
    let max_chars = (width / (6 * scale.max(1))).max(1);
    let words = value.split_whitespace().collect::<Vec<_>>();
    if words.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    for word in words {
        let proposed_len = if current.is_empty() {
            word.chars().count()
        } else {
            current.chars().count() + 1 + word.chars().count()
        };
        if proposed_len <= max_chars {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
            continue;
        }
        if !current.is_empty() {
            lines.push(current);
            current = String::new();
            if lines.len() == max_lines {
                break;
            }
        }
        if word.chars().count() > max_chars {
            lines.push(truncate(word, max_chars));
            if lines.len() == max_lines {
                break;
            }
        } else {
            current.push_str(word);
        }
    }
    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    }
    lines
}

fn primary_date(job: &LabelJob) -> Option<(&'static str, &str)> {
    job.expires_on
        .as_deref()
        .map(|date| ("USE BY", date))
        .or_else(|| job.produced_on.as_deref().map(|date| ("MADE", date)))
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

    fn draw_branded_qr(
        &mut self,
        qr: &QrCode,
        x: usize,
        y: usize,
        actual_qr_px: usize,
        module_px: usize,
    ) {
        self.draw_qr(qr, x, y, module_px);
        let badge_px = branded_qr_badge_px(actual_qr_px, module_px);
        let badge_x = x + (actual_qr_px.saturating_sub(badge_px)) / 2;
        let badge_y = y + (actual_qr_px.saturating_sub(badge_px)) / 2;
        self.draw_rect(badge_x, badge_y, badge_px, badge_px, Pixel::White);
        let border = (module_px / 2).max(2);
        self.draw_rect(badge_x, badge_y, badge_px, border, Pixel::Black);
        self.draw_rect(
            badge_x,
            badge_y + badge_px.saturating_sub(border),
            badge_px,
            border,
            Pixel::Black,
        );
        self.draw_rect(badge_x, badge_y, border, badge_px, Pixel::Black);
        self.draw_rect(
            badge_x + badge_px.saturating_sub(border),
            badge_y,
            border,
            badge_px,
            Pixel::Black,
        );
        let logo_px = branded_qr_logo_px(actual_qr_px, module_px);
        let logo_x = x + (actual_qr_px.saturating_sub(logo_px)) / 2;
        let logo_y = y + (actual_qr_px.saturating_sub(logo_px)) / 2;
        self.draw_quartermaster_mark(logo_x, logo_y, logo_px);
    }

    fn draw_quartermaster_mark(&mut self, x: usize, y: usize, size: usize) {
        let outer_x = x + size * 15 / 100;
        let outer_y = y + size * 12 / 100;
        let outer_w = size * 70 / 100;
        let outer_h = size * 58 / 100;
        let stroke = (size / 8).max(3);
        self.draw_rect(outer_x, outer_y, outer_w, outer_h, Pixel::Black);
        self.draw_rect(
            outer_x + stroke,
            outer_y + stroke,
            outer_w.saturating_sub(stroke * 2),
            outer_h.saturating_sub(stroke * 2),
            Pixel::White,
        );
        self.draw_rect(
            outer_x + outer_w * 28 / 100,
            outer_y + outer_h * 50 / 100,
            outer_w * 44 / 100,
            stroke.max(2),
            Pixel::Black,
        );
        self.draw_thick_line(
            outer_x + outer_w * 62 / 100,
            outer_y + outer_h * 72 / 100,
            x + size * 86 / 100,
            y + size * 90 / 100,
            stroke,
            Pixel::Black,
        );
    }

    fn draw_thick_line(
        &mut self,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
        thickness: usize,
        pixel: Pixel,
    ) {
        let mut x0 = x0 as isize;
        let mut y0 = y0 as isize;
        let x1 = x1 as isize;
        let y1 = y1 as isize;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let radius = (thickness / 2) as isize;

        loop {
            for yy in y0 - radius..=y0 + radius {
                for xx in x0 - radius..=x0 + radius {
                    if xx >= 0 && yy >= 0 {
                        self.set(xx as usize, yy as usize, pixel);
                    }
                }
            }
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
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

fn truncate_for_width(value: &str, width: usize, scale: usize) -> String {
    truncate(value, (width / (6 * scale.max(1))).max(1))
}

fn text_width(value: &str, scale: usize) -> usize {
    value.chars().flat_map(char::to_uppercase).count() * 6 * scale.max(1)
}

fn line_height(scale: usize) -> usize {
    7 * scale.max(1)
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
            produced_on: Some("2026-05-01".into()),
            expires_on: Some("2026-06-01".into()),
            opened_on: None,
            note: Some("bag".into()),
            include_quantity: false,
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
            produced_on: Some("2026-05-01".into()),
            expires_on: Some("2026-06-01".into()),
            opened_on: None,
            note: Some("bag".into()),
            include_quantity: false,
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
            produced_on: None,
            expires_on: None,
            opened_on: None,
            note: None,
            include_quantity: false,
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

    #[test]
    fn label_qr_uses_high_correction_with_centered_mark() {
        let qr = batch_qr(
            "https://quartermaster.example.com/batches/33333333-3333-3333-3333-333333333333",
        )
        .unwrap();
        assert_eq!(qr.error_correction_level(), EcLevel::H);

        let (actual_qr_px, module_px) = qr_size(&qr, 300);
        let logo_px = branded_qr_logo_px(actual_qr_px, module_px);
        let badge_px = branded_qr_badge_px(actual_qr_px, module_px);
        let badge_x = (actual_qr_px - badge_px) / 2;
        let badge_y = (actual_qr_px - badge_px) / 2;
        let logo_x = (actual_qr_px - logo_px) / 2;
        let logo_y = (actual_qr_px - logo_px) / 2;
        let mut bitmap = Bitmap::new(actual_qr_px, actual_qr_px);
        bitmap.draw_branded_qr(&qr, 0, 0, actual_qr_px, module_px);

        assert_eq!(bitmap.get(badge_x, badge_y), Pixel::Black);
        assert_eq!(
            bitmap.get(badge_x + module_px, badge_y + module_px),
            Pixel::White
        );
        assert_eq!(
            bitmap.get(logo_x + logo_px * 25 / 100, logo_y + logo_px * 20 / 100),
            Pixel::Black
        );
        assert_eq!(
            bitmap.get(logo_x + logo_px / 2, logo_y + logo_px * 30 / 100),
            Pixel::White
        );
    }

    #[test]
    fn product_name_wraps_into_large_label_headline() {
        assert_eq!(
            wrap_for_width("Red Pepper Flakes", 360, 5, 2),
            vec!["Red Pepper", "Flakes"]
        );
    }

    #[test]
    fn label_quantity_omits_location() {
        let job = LabelJob {
            batch_id: Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap(),
            batch_url:
                "https://quartermaster.example.com/batches/33333333-3333-3333-3333-333333333333"
                    .into(),
            product_name: "Sofrito".into(),
            brand: None,
            location_name: "Freezer".into(),
            quantity: "2".into(),
            unit: "portions".into(),
            produced_on: None,
            expires_on: None,
            opened_on: None,
            note: None,
            include_quantity: true,
        };

        assert_eq!(label_quantity(&job), "2 portions");
    }

    #[test]
    fn primary_label_date_prefers_expiry_then_production() {
        let mut job = LabelJob {
            batch_id: Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap(),
            batch_url:
                "https://quartermaster.example.com/batches/33333333-3333-3333-3333-333333333333"
                    .into(),
            product_name: "Sofrito".into(),
            brand: None,
            location_name: "Freezer".into(),
            quantity: "2".into(),
            unit: "portion".into(),
            produced_on: Some("2026-05-01".into()),
            expires_on: None,
            opened_on: None,
            note: None,
            include_quantity: false,
        };

        assert_eq!(primary_date(&job), Some(("MADE", "2026-05-01")));

        job.expires_on = Some("2026-06-01".into());
        assert_eq!(primary_date(&job), Some(("USE BY", "2026-06-01")));
    }
}
