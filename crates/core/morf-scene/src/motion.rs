use crate::motion_values::*;
use animato::{Tween, TweenState};

use crate::{animation::*, types::*};

mod physics;

pub(crate) use physics::*;

impl Animation {
    pub(crate) fn new(
        from: Value,
        to: Value,
        initial_velocity: Velocity,
        preserve_velocity: bool,
        behavior: Behavior,
    ) -> Self {
        let clock = Tween::new(0.0, 1.0)
            .duration(behavior.duration.as_secs_f32())
            .easing(behavior.easing.animato())
            .delay(behavior.delay.as_secs_f32())
            .time_scale(behavior.time_scale.max(0.0) as f32)
            .looping(behavior.repeat.animato())
            .build();
        Self {
            from,
            to,
            initial_velocity,
            preserve_velocity,
            clock,
            behavior,
            fresh: true,
        }
    }

    pub(crate) fn progress(&self) -> f64 {
        let progress = f64::from(self.clock.progress());
        if self.clock.is_ping_pong_reversed() {
            1.0 - progress
        } else {
            progress
        }
    }

    /// Reports whether the interval is waiting out its behavior delay.
    pub(crate) fn is_delayed(&self) -> bool {
        matches!(self.clock.state(), TweenState::Idle)
    }

    /// Reports whether playback is halted without having reached the target.
    pub(crate) fn is_paused(&self) -> bool {
        matches!(self.clock.state(), TweenState::Paused)
    }

    /// Reports whether the animation settles on its own.
    pub(crate) fn settles(&self) -> bool {
        !self.behavior.repeat.is_endless()
    }

    /// The value a settling animation comes to rest on.
    ///
    /// An alternating repetition that ends on a backward pass finishes where it
    /// started, so the resting value is not always the target.
    pub(crate) fn settled(&self) -> &Value {
        if self.clock.is_ping_pong_reversed() {
            &self.from
        } else {
            &self.to
        }
    }

    pub(crate) fn value(&self) -> Value {
        let progress = if self.preserve_velocity {
            self.progress()
        } else if let Easing::Spline(points) = self.behavior.easing {
            // The clock was built linear for a spline; the curve goes on here,
            // on the raw progress and before a backward pass mirrors it, which
            // is what the clock does with an easing of its own.
            let eased = crate::spline::spline_value(points, f64::from(self.clock.progress()));
            if self.clock.is_ping_pong_reversed() {
                1.0 - eased
            } else {
                eased
            }
        } else {
            f64::from(self.clock.value())
        };
        if self.preserve_velocity {
            interpolate_hermite(
                &self.from,
                &self.to,
                self.initial_velocity,
                self.behavior.duration.as_secs_f64(),
                progress,
                self.behavior.color_space,
                self.behavior.hue,
            )
        } else {
            interpolate_in(
                &self.from,
                &self.to,
                progress,
                self.behavior.color_space,
                self.behavior.hue,
            )
        }
    }

    pub(crate) fn velocity(&self) -> Velocity {
        let duration = self.behavior.duration.as_secs_f64();
        if duration == 0.0 {
            return zero_velocity(&self.to);
        }
        let progress = self.progress();
        let epsilon = (1.0 / (duration * 1_000.0)).clamp(1e-6, 1e-3);
        let before = (progress - epsilon).max(0.0);
        let after = (progress + epsilon).min(1.0);
        let span = (after - before) * duration;
        let (space, hue) = (self.behavior.color_space, self.behavior.hue);
        let before_value = if self.preserve_velocity {
            interpolate_hermite(
                &self.from,
                &self.to,
                self.initial_velocity,
                duration,
                before,
                space,
                hue,
            )
        } else {
            interpolate_in(
                &self.from,
                &self.to,
                self.behavior.easing.value_at(before),
                space,
                hue,
            )
        };
        let after_value = if self.preserve_velocity {
            interpolate_hermite(
                &self.from,
                &self.to,
                self.initial_velocity,
                duration,
                after,
                space,
                hue,
            )
        } else {
            interpolate_in(
                &self.from,
                &self.to,
                self.behavior.easing.value_at(after),
                space,
                hue,
            )
        };
        value_velocity(&before_value, &after_value, span, space, hue)
    }
}

pub(crate) fn interpolatable(from: &Value, to: &Value) -> bool {
    match (from, to) {
        (Value::Number(_), Value::Number(_)) | (Value::Color(_), Value::Color(_)) => true,
        // A tree of values — a gradient's stops — moves when the two trees
        // have the same shape and every leaf can.
        (Value::List(from), Value::List(to)) => {
            from.len() == to.len()
                && from
                    .iter()
                    .zip(to)
                    .all(|(from, to)| interpolatable(from, to) || from == to)
        }
        (Value::Map(from), Value::Map(to)) => {
            from.keys().eq(to.keys())
                && from
                    .values()
                    .zip(to.values())
                    .all(|(from, to)| interpolatable(from, to) || from == to)
        }
        _ => false,
    }
}

pub(crate) fn animation_start(
    property: &str,
    from: Value,
    to: &Value,
    direction: RotationDirection,
) -> Value {
    let (Value::Number(from_number), Value::Number(to_number)) = (&from, to) else {
        return from;
    };
    if property != "rotation" {
        return from;
    }
    let delta = match direction {
        RotationDirection::Numerical => return from,
        RotationDirection::Shortest => (to_number - from_number + 180.0).rem_euclid(360.0) - 180.0,
        RotationDirection::Clockwise => (to_number - from_number).rem_euclid(360.0),
        RotationDirection::CounterClockwise => -((from_number - to_number).rem_euclid(360.0)),
    };
    Value::Number(to_number - delta)
}

/// Whether layout reads this property.
///
/// Deliberately not [`property_class`]. That answers "what work does a change
/// need", and it calls `x` a transform because the renderer offsets by it —
/// but layout bakes `x` into the geometry it produces, and reads `border_width`
/// for a `ClipRect`'s content inset even though painting owns the border. Using
/// it here would let a moved node keep a stale layout.
///
/// The list is negative on purpose: everything counts unless it is known never
/// to be read, so a property added to the schema without a thought here costs
/// one extra layout pass rather than a frame drawn at the wrong geometry.
pub(crate) fn affects_layout(property: &str) -> bool {
    !matches!(
        property,
        "scale"
            | "scale_x"
            | "scale_y"
            | "skew_x"
            | "skew_y"
            | "translate_x"
            | "translate_y"
            | "transform_origin_x"
            | "transform_origin_y"
            | "transform_matrix"
            | "matrix"
            | "blend_group"
            | "blend_profile"
            | "rotation"
            | "opacity"
            | "color"
            | "color_overlay"
            | "layer"
            | "radius"
            | "top_left_radius"
            | "top_right_radius"
            | "bottom_right_radius"
            | "bottom_left_radius"
            | "border_color"
            | "antialiasing"
            | "border_pixel_aligned"
            | "content_under_border"
            | "gradient"
            | "decoration"
            | "cursor"
            | "blur"
            | "backdrop_blur"
            | "backdrop_saturation"
            | "shadow_color"
            | "shadow_blur"
            | "shadow_spread"
            | "shadow_offset_x"
            | "shadow_offset_y"
            | "shadow_inner"
            | "morph_progress"
            | "blend"
            | "thickness"
            | "softness"
            | "outline_width"
            | "outline_color"
            | "fill_color"
            | "stroke_color"
            // A path's outline and how it is drawn are all inside the box it
            // was given; only its view box says how big that box wants to be.
            | "d"
            | "morph_to"
            | "fill_rule"
            | "stroke_width"
            | "stroke_cap"
            | "stroke_join"
            | "miter_limit"
            | "dash"
            | "dash_offset"
            | "trim_start"
            | "trim_end"
            // A text input's caret, selection and scroll are where it is
            // looking, not how big it is: they move on every key, and each of
            // them costing a layout pass would be a layout per keystroke.
            | "cursor_position"
            | "selection_start"
            | "selection_end"
            | "scroll_x"
            | "scroll_y"
            | "content_width"
            | "content_height"
            | "caret_visible"
            | "caret_color"
            | "caret_width"
            | "selection_color"
            | "selected_text_color"
            | "placeholder_color"
            | "focus"
            | "tab_navigation"
            | "hovered"
            | "pressed"
            // An image's status and playback say what is drawn in its box,
            // never how big the box is.
            | "status"
            | "error"
            | "playing"
            | "speed"
            | "frame"
            | "loops"
            | "frame_count"
            | "links"
            | "link_color"
    ) && !property.starts_with("accessible_")
}
