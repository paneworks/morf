//! Aiming a property at a new target by a motion the caller names rather
//! than the one the property declares: the current value and its speed
//! carried across, whichever kind of motion was running before.

use crate::{animation::*, motion::*, motion_values::*, schema::*, types::*};

impl Scene {
    /// Aims `property` at `value` by `motion` from where it is now, with
    /// the speed it has -- a spring taking over a tween, a tween taking
    /// over a spring, or either taking over its own kind. The motion moves
    /// this one change; the behaviour the property declares stays as it
    /// was. `None` is a plain [`Scene::assign`], through that behaviour.
    pub fn retarget(
        &mut self,
        node: NodeHandle,
        property: &str,
        value: impl Into<Value>,
        motion: Option<Motion>,
    ) -> Result<(), SceneError> {
        let Some(motion) = motion else {
            return self.assign(node, property, value);
        };
        let (key, slot) = self.property_key(node, property)?;
        let element = self.nodes[key.node].element;
        let value = coerce(element, property, slot.kind, value.into())?;
        if self.properties.read(slot.target)? == &value {
            return Ok(());
        }
        let current = self.properties.read(slot.current)?.clone();
        let velocity = self.running_velocity(key, &current);
        let started = match (motion, &current, &value) {
            (Motion::Physics(spec), Value::Number(from), Value::Number(to)) => {
                self.check_physics(spec)?;
                let carried = match velocity {
                    Velocity::Number(velocity) => velocity,
                    Velocity::Color(_) => 0.0,
                };
                self.animations.remove(&key);
                self.paused_physics.remove(&key);
                let motion = physics_animation(*from, *to, carried, spec);
                self.physics.insert(key, motion);
                true
            }
            (Motion::Physics(spec), Value::Color(from), Value::Color(to)) => {
                self.check_physics(spec)?;
                let carried = match velocity {
                    Velocity::Color(velocity) => velocity,
                    Velocity::Number(_) => [0.0; 4],
                };
                self.animations.remove(&key);
                self.paused_physics.remove(&key);
                let motion = physics_animation_color(*from, *to, carried, spec);
                self.physics.insert(key, motion);
                true
            }
            (Motion::Tween(behavior), _, _)
                if behavior.intercepts() && interpolatable(&current, &value) =>
            {
                let from =
                    animation_start(key.property, current, &value, behavior.rotation_direction);
                let carried = match velocity {
                    Velocity::Number(_) if behavior.keep_velocity => velocity,
                    Velocity::Color(_)
                        if behavior.keep_velocity
                            && behavior.color_space == crate::color::ColorSpace::Oklab =>
                    {
                        velocity
                    }
                    _ => zero_velocity(&from),
                };
                self.physics.remove(&key);
                self.paused_physics.remove(&key);
                let animation =
                    Animation::new(from, value.clone(), carried, carried.is_moving(), behavior);
                self.animations.insert(key, animation);
                true
            }
            _ => false,
        };
        if started {
            self.properties.write(slot.target, value)?;
        } else {
            let interrupted =
                self.animations.remove(&key).is_some() | self.physics.remove(&key).is_some();
            self.paused_physics.remove(&key);
            if interrupted {
                self.push_event(key, AnimationEnd::Canceled);
            }
            self.properties.batch(|graph| {
                graph.write(slot.target, value.clone())?;
                graph.write(slot.current, value)?;
                Ok(())
            })?;
        }
        self.touch_layout(key.node, key.property);
        Ok(())
    }

    /// The motion a property declares for its writes, if any: its
    /// behaviour or its physics.
    pub fn declared_motion(&self, node: NodeHandle, property: &str) -> Option<Motion> {
        let (key, _) = self.property_key(node, property).ok()?;
        if let Some(physics) = self.physics_specs.get(&key) {
            return Some(Motion::Physics(*physics));
        }
        self.behaviors.get(&key).copied().map(Motion::Tween)
    }

    /// `value` as `property` of `node` would hold it, or why it cannot.
    pub fn coerce_value(
        &self,
        node: NodeHandle,
        property: &str,
        value: Value,
    ) -> Result<Value, SceneError> {
        let (key, slot) = self.property_key(node, property)?;
        coerce(self.nodes[key.node].element, property, slot.kind, value)
    }

    /// Whether `motion` can move `property` of `node`: physics moves only
    /// numbers and colours, and never as a coast.
    pub fn check_motion(
        &self,
        node: NodeHandle,
        property: &str,
        motion: Motion,
    ) -> Result<(), SceneError> {
        let (key, slot) = self.property_key(node, property)?;
        let Motion::Physics(physics) = motion else {
            return Ok(());
        };
        if !matches!(slot.kind, PropertyType::Number | PropertyType::Color) {
            return Err(SceneError::InvalidPropertyType {
                element: self.nodes[key.node].element.name(),
                property: property.to_owned(),
                expected: "numeric or colour property",
            });
        }
        self.check_physics(physics)
    }

    /// How fast a numeric property is moving now, in units a second.
    pub fn velocity(&self, node: NodeHandle, property: &str) -> Result<f64, SceneError> {
        let (key, slot) = self.property_key(node, property)?;
        let current = self.properties.read(slot.current)?;
        Ok(match self.running_velocity(key, current) {
            Velocity::Number(velocity) => velocity,
            Velocity::Color(_) => 0.0,
        })
    }

    fn check_physics(&self, physics: Physics) -> Result<(), SceneError> {
        if matches!(physics, Physics::Decay { .. }) {
            return Err(SceneError::Reactive(
                "decay is started with a fling, not aimed at a target".to_owned(),
            ));
        }
        validate_physics(physics).map_err(SceneError::Reactive)
    }

    /// The speed of whatever moves a property now, zero at rest. A
    /// colour's is in OkLab, the space physics moves colours in; a tween
    /// through another space hands over none.
    fn running_velocity(&self, key: PropertyKey, current: &Value) -> Velocity {
        if let Some(physics) = self.physics.get(&key)
            && !self.paused_physics.contains(&key)
        {
            return match physics {
                PhysicsAnimation::Color { .. } => Velocity::Color(physics.color_velocity()),
                _ => Velocity::Number(physics.velocity()),
            };
        }
        match self.animations.get(&key) {
            Some(animation) if !animation.is_paused() && !animation.is_delayed() => {
                match animation.velocity() {
                    Velocity::Color(_)
                        if animation.behavior.color_space != crate::color::ColorSpace::Oklab =>
                    {
                        zero_velocity(current)
                    }
                    velocity => velocity,
                }
            }
            _ => zero_velocity(current),
        }
    }
}
