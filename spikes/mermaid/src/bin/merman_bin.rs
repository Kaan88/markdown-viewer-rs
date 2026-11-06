#[path = "../common.rs"]
mod common;
use merman::svg::SvgPipeline;
use merman::{OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};

fn render(src: &str) -> Result<String, String> {
    // Renderer::new() keeps merman's default "interactive" input limits.
    let req = SvgRequest { pipeline: Some(SvgPipeline::resvg_safe()), ..Default::default() };
    match Renderer::new().render(RenderRequest::svg(src, OperationControl::new(), req)) {
        Ok(RenderOutput::Svg(Some(s))) => Ok(s.svg().to_string()),
        Ok(_) => Err("no diagram detected".into()),
        Err(e) => Err(e.to_string()),
    }
}

fn main() {
    let name = if cfg!(feature = "r-merman-cose") { "merman-cose" } else { "merman" };
    common::run(name, render);
}
