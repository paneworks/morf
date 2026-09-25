// What the layout engine asks of text: how big a string comes out, given a
// font, a size and the room it has.

use cosmic_text::{Align, Buffer, Shaping, Wrap};
use morf_layout::{Size, TextAlignment, TextMeasurer, TextOptions};
use morf_scene::NodeHandle;

use crate::style::{run_gain, shaping_weight, text_attrs, text_metrics};
use crate::{BufferKey, CachedBuffer, TextInput, TextSystem, elided_text, resolve_family};

impl TextSystem {
    /// Shapes and measures the text a node is morphing towards.
    ///
    /// The same work as measuring the node's own text, against the node's other
    /// buffer. It has to be shaped for the morph to have anything to aim at:
    /// the interpolation is between two sets of glyphs, and the target's are
    /// only known once it has been through the shaper.
    pub fn measure_target(
        &mut self,
        node: NodeHandle,
        text: &str,
        family: &str,
        size: f64,
        options: TextOptions,
    ) -> Size {
        self.shape(BufferKey::target(node), text, family, size, options)
    }

    fn shape(
        &mut self,
        key: BufferKey,
        text: &str,
        family: &str,
        size: f64,
        options: TextOptions,
    ) -> Size {
        self.load_font_source(options.font_source.as_deref());
        let size = size.max(1.0) as f32;
        // A `wght` among the axes is the weight; the other axes are shaped as
        // variations, and the rasteriser follows both.
        let font_weight = shaping_weight(&options);
        let input = TextInput {
            text: text.to_owned(),
            family: family.to_owned(),
            size: (size as f64).to_bits(),
            width: options.width.map(f64::to_bits),
            wrap: options.wrap,
            alignment: options.alignment,
            elide: options.elide,
            font_weight,
            font_source: options.font_source.clone(),
            max_lines: options.max_lines,
            style: options.style.key(),
        };
        let metrics = text_metrics(size, &options.style);
        let cached = self.buffers.entry(key).or_insert_with(|| CachedBuffer {
            buffer: Buffer::new(&mut self.fonts, metrics),
            input: None,
            word_spacing: 0.0,
            alignment: options.alignment,
            rich: None,
            axes: Vec::new(),
            optical: false,
        });
        if cached.input.as_ref() != Some(&input) {
            cached.axes = options.style.variation_axes(size);
            cached.optical = options.style.optical_sizing == morf_layout::OpticalSizing::Auto
                && options.style.axis(b"opsz").is_none();
            cached.buffer.set_metrics_and_size(
                metrics,
                options.width.map(|value| value as f32),
                None,
            );
            cached.buffer.set_wrap(if options.wrap {
                Wrap::WordOrGlyph
            } else {
                Wrap::None
            });
            let rich = options.style.rich.clone();
            let source = rich.as_ref().map_or(text, |rich| rich.text.as_str());
            let displayed = elided_text(&mut self.fonts, source, family, size, &options);
            let family = resolve_family(&self.fonts, family);
            cached.word_spacing = options.style.word_spacing as f32;
            cached.alignment = options.alignment;
            let align = Some(match options.alignment {
                TextAlignment::Left => Align::Left,
                TextAlignment::Right => Align::Right,
                TextAlignment::Center => Align::Center,
                TextAlignment::Justified => Align::Justified,
            });
            let base = text_attrs(&family, font_weight, size, &options.style);
            match &rich {
                None => cached
                    .buffer
                    .set_text(&displayed, &base, Shaping::Advanced, align),
                Some(rich) => {
                    // Each run's family resolved once, and kept alive for the
                    // attributes that borrow it.
                    let families: Vec<_> = rich
                        .spans
                        .iter()
                        .map(|span| {
                            span.family
                                .as_deref()
                                .map(|name| resolve_family(&self.fonts, name))
                        })
                        .collect();
                    let segments = crate::rich::segments(rich, &displayed);
                    let runs = segments.iter().map(|(range, index)| {
                        let attrs = crate::rich::span_attrs(
                            &base,
                            &rich.spans[*index],
                            *index,
                            families[*index].as_ref(),
                            size,
                            &options.style,
                        );
                        (&displayed[range.clone()], attrs)
                    });
                    cached
                        .buffer
                        .set_rich_text(runs, &base, Shaping::Advanced, align);
                }
            }
            cached.rich = rich;
            cached.buffer.shape_until_scroll(&mut self.fonts, false);
            cached.input = Some(input);
        }

        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        for run in cached.buffer.layout_runs() {
            width = width.max(run.line_w + run_gain(&run, cached.word_spacing));
            height = height.max(run.line_top + run.line_height);
        }
        Size {
            width: width as f64,
            height: height as f64,
        }
    }
}

impl TextMeasurer for TextSystem {
    fn measure(
        &mut self,
        node: NodeHandle,
        text: &str,
        family: &str,
        size: f64,
        options: TextOptions,
    ) -> Size {
        self.shape(BufferKey::own(node), text, family, size, options)
    }
}
