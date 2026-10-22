//! Replaces Typst's HTML image show rule with one that emits `rheo-asset:`
//! placeholders naming the project path, instead of inlining base64, when
//! `[html] image_assets` is on. Installed onto a world's library by
//! `crate::world::RheoWorld::assemble`.
//!
//! Wraps the stock rule rather than reimplementing it: its CSS sizing
//! (width/height, `image-rendering`) goes through typst-html's private `css`
//! module, which rheo cannot rebuild. There is also no side store of image
//! bytes here — the placeholder carries everything needed — because comemo
//! can serve a cached show-rule result without re-running the rule, which
//! would leave a store filled by the rule incomplete on a warm rebuild.

use std::sync::OnceLock;

use ecow::EcoString;
use typst::diag::SourceResult;
use typst::engine::Engine;
use typst::foundations::{Content, NativeRuleMap, NativeShowRule, StyleChain, Target};
use typst::layout::{BlockBody, BlockElem};
use typst::loading::LoadSource;
use typst::syntax::VirtualRoot;
use typst::visualize::{ExchangeFormat, ImageElem, ImageFormat, RasterFormat, VectorFormat};
use typst_html::attr;

/// Scheme prefix a placeholder `src` carries in place of a base64 data URL.
/// The sibling bird that turns placeholders into page-relative URLs reads
/// this same constant.
pub const PLACEHOLDER_SCHEME: &str = "rheo-asset:";

/// Whether a decoded image's format is one a browser can show directly,
/// so an `<img src>` naming the project path is enough once the sibling
/// bird publishes it as a file — no transcoding involved.
fn is_web_native(format: ImageFormat) -> bool {
    matches!(
        format,
        ImageFormat::Raster(RasterFormat::Exchange(
            ExchangeFormat::Png | ExchangeFormat::Jpg | ExchangeFormat::Gif | ExchangeFormat::Webp
        )) | ImageFormat::Vector(VectorFormat::Svg)
    )
}

/// Typst's own `Target::Html` rule for `ImageElem`, fetched once and cached:
/// building the full native rule map just to pull one rule out of it is
/// wasteful to repeat per image.
fn stock_image_rule(elem: &Content) -> NativeShowRule {
    static RULE: OnceLock<NativeShowRule> = OnceLock::new();
    *RULE.get_or_init(|| {
        let mut map = NativeRuleMap::new();
        typst_html::register(&mut map);
        map.get(Target::Html, elem)
            .expect("typst-html registers a Target::Html rule for ImageElem")
    })
}

/// The project path to ferry an image's bytes under, or `None` when the
/// image should keep the stock base64 rule — a package image (not this
/// project's to publish), a byte source (no project path to name), or a
/// format a browser cannot show natively (PDF, raw pixels).
fn ferryable_path(
    elem: &typst::foundations::Packed<ImageElem>,
    engine: &mut Engine,
    styles: StyleChain,
) -> SourceResult<Option<EcoString>> {
    let LoadSource::Path(id) = elem.source.derived.source.v else {
        return Ok(None);
    };
    if !matches!(id.root(), VirtualRoot::Project) {
        return Ok(None);
    }

    let image = elem.decode(engine, styles)?;
    if !is_web_native(image.format()) {
        return Ok(None);
    }

    Ok(Some(id.vpath().get_with_slash().into()))
}

/// Show rule installed in place of typst-html's `Target::Html` rule for
/// `ImageElem` when `[html] image_assets` is on. Produces exactly the stock
/// rule's output, except a ferryable image's `src` names a `rheo-asset:`
/// placeholder instead of carrying inlined base64.
pub const IMAGE_PLACEHOLDER_RULE: typst::foundations::ShowFn<ImageElem> = |elem, engine, styles| {
    let stock = stock_image_rule(elem.pack_ref());
    let mut out = stock.apply(elem.pack_ref(), engine, styles)?;

    let Some(path) = ferryable_path(elem, engine, styles)? else {
        return Ok(out);
    };
    let placeholder: EcoString = format!("{PLACEHOLDER_SCHEME}{path}").into();

    if let Some(block) = out.to_packed_mut::<BlockElem>()
        && let Some(Some(BlockBody::Content(inner))) = block.body.as_option_mut()
        && let Some(html) = inner.to_packed_mut::<typst_html::HtmlElem>()
        && let Some(attrs) = html.attrs.as_option_mut()
    {
        for (name, value) in attrs.0.make_mut().iter_mut() {
            if *name == attr::src {
                *value = placeholder.clone();
            }
        }
    }

    Ok(out)
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_web_native_accepts_browser_formats() {
        assert!(is_web_native(ImageFormat::Raster(RasterFormat::Exchange(
            ExchangeFormat::Png
        ))));
        assert!(is_web_native(ImageFormat::Raster(RasterFormat::Exchange(
            ExchangeFormat::Jpg
        ))));
        assert!(is_web_native(ImageFormat::Raster(RasterFormat::Exchange(
            ExchangeFormat::Gif
        ))));
        assert!(is_web_native(ImageFormat::Raster(RasterFormat::Exchange(
            ExchangeFormat::Webp
        ))));
        assert!(is_web_native(ImageFormat::Vector(VectorFormat::Svg)));
        assert!(!is_web_native(ImageFormat::Vector(VectorFormat::Pdf)));
    }
}
