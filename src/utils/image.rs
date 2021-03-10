use crate::Weechat2;
use anyhow::Context;
use image::{DynamicImage, ImageFormat};
use tokio::runtime::Runtime;
use twilight_model::channel::Message;

pub struct InlineImageCandidate {
    pub url: String,
    pub height: u64,
    pub width: u64,
}

pub fn find_image_candidates(msg: &Message) -> Vec<InlineImageCandidate> {
    let mut out = Vec::new();

    for attachment in &msg.attachments {
        if ImageFormat::from_path(&attachment.proxy_url).is_ok() {
            out.push(InlineImageCandidate {
                url: attachment.proxy_url.clone(),
                height: attachment.height.unwrap_or(900),
                width: attachment.width.unwrap_or(20),
            });
        }
    }

    for embed in &msg.embeds {
        if let Some(thumbnail) = embed.thumbnail.as_ref() {
            if let Some(url) = thumbnail.proxy_url.as_ref() {
                out.push(InlineImageCandidate {
                    url: url.clone(),
                    height: thumbnail.height.unwrap_or(900 * 2) / 2,
                    width: thumbnail.width.unwrap_or(20 * 2) / 2,
                });
            }
        }
    }

    out
}

/// Wraps reqwest fetch so this function can be called on a weechat future
pub async fn fetch_inline_image(rt: &Runtime, url: &str) -> anyhow::Result<DynamicImage> {
    let url = url.to_owned();
    rt.spawn(async move {
        tracing::trace!("Fetching inline image at: {}", url);

        let client = hyper::Client::builder()
            .build::<_, hyper::Body>(hyper_rustls::HttpsConnector::with_native_roots());

        let uri = url.parse().expect("Discord sent an invalid uri");
        let response = client
            .get(uri)
            .await
            .with_context(|| format!("Failed to fetch image url: {}", url))?;
        let body = hyper::body::to_bytes(response)
            .await
            .context("Failed to fetch image asset body")?;

        tracing::trace!("Successfully loaded image");
        image::load_from_memory(body.as_ref()).context("Failed to load image")
    })
    .await
    .expect("Task is never aborted")
}

pub fn render_img(img: &DynamicImage) -> String {
    let render = termimage::render(img, true, 2);

    let mut out = String::new();

    for x in render {
        for y in x {
            let fg = termimage::rgb_to_ansi(y.fg).0;
            let bg = termimage::rgb_to_ansi(y.bg).0;
            out.push_str(&format!(
                "{}{}{}",
                Weechat2::color(&format!("{},{}", fg, bg)),
                y.ch,
                Weechat2::color("reset")
            ));
        }
        out.push('\n');
    }

    out
}

/// Resizes an image to fit within a max size, then scales an image to fit within a block size
pub fn resize_image(
    img: &DynamicImage,
    cell_size: (u32, u32),
    max_size: (u32, u32),
) -> DynamicImage {
    use image::GenericImageView;
    let img = img.resize(
        max_size.0 * cell_size.0,
        max_size.1 * cell_size.1,
        image::imageops::FilterType::Nearest,
    );

    img.resize_exact(
        closest_mult(img.width(), cell_size.0),
        closest_mult(img.height(), cell_size.1),
        image::imageops::FilterType::Nearest,
    )
}

/// Returns the closest multiple of a base
fn closest_mult(x: u32, base: u32) -> u32 {
    base * ((x as f32) / base as f32).round() as u32
}
