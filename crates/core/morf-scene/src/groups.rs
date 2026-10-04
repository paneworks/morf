use std::time::Duration;

use crate::{animation::*, types::*};

/// Handle to a group started on the scene.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GroupId(u64);

/// One leg of an animation group.
///
/// A group schedules ordinary property animations; it does not animate anything
/// itself. Once a step starts, the property it targets is driven by the same
/// behavior machinery a direct write uses, so retargeting, damage classification
/// and completion events all continue to apply to it.
#[derive(Clone, Debug, PartialEq)]
pub enum AnimationStep {
    /// Animate one property to a target over a behavior's interval.
    Property {
        /// Node owning the property.
        node: NodeHandle,
        /// Property name, which must exist on the node's element.
        property: String,
        /// Explicit start value, or the property's own value when the step runs.
        from: Option<Value>,
        /// Value the step animates towards.
        to: Value,
        /// Timing for this step.
        behavior: Behavior,
    },
    /// Occupy time without changing anything.
    Pause(Duration),
    /// Run the children one after another.
    Sequential(Vec<AnimationStep>),
    /// Run the children together, finishing when the longest one does.
    Parallel(Vec<AnimationStep>),
}

impl AnimationStep {
    /// How long the step occupies its parent's timeline.
    pub(crate) fn duration(&self) -> Duration {
        match self {
            Self::Property { behavior, .. } => {
                behavior.delay + behavior.duration * passes(*behavior)
            }
            Self::Pause(duration) => *duration,
            Self::Sequential(steps) => steps.iter().map(Self::duration).sum(),
            Self::Parallel(steps) => steps
                .iter()
                .map(Self::duration)
                .max()
                .unwrap_or(Duration::ZERO),
        }
    }

    /// Rejects the shapes a group cannot schedule.
    ///
    /// A step that never settles has no end for the rest of the group to wait
    /// on, so an endless repetition is refused here rather than silently
    /// stalling everything after it.
    pub(crate) fn validate(&self) -> Result<(), SceneError> {
        match self {
            Self::Property {
                property, behavior, ..
            } if behavior.repeat.is_endless() => Err(SceneError::Reactive(format!(
                "animation group step `{property}` cannot repeat endlessly"
            ))),
            Self::Property { .. } | Self::Pause(_) => Ok(()),
            Self::Sequential(steps) | Self::Parallel(steps) => {
                steps.iter().try_for_each(Self::validate)
            }
        }
    }

    /// Flattens the tree into absolute start times measured from the group start.
    pub(crate) fn schedule(&self, at: Duration, out: &mut Vec<ScheduledStep>) {
        match self {
            Self::Property {
                node,
                property,
                from,
                to,
                behavior,
            } => out.push(ScheduledStep {
                at,
                node: *node,
                property: property.clone(),
                from: from.clone(),
                to: to.clone(),
                behavior: *behavior,
            }),
            Self::Pause(_) => {}
            Self::Sequential(steps) => {
                let mut cursor = at;
                for step in steps {
                    step.schedule(cursor, out);
                    cursor += step.duration();
                }
            }
            Self::Parallel(steps) => {
                for step in steps {
                    step.schedule(at, out);
                }
            }
        }
    }
}

/// A group that ended, reported alongside the properties that changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroupEvent {
    /// Group the event belongs to.
    pub group: GroupId,
    /// Why the group ended.
    pub end: AnimationEnd,
}

/// One property step resolved to an absolute offset from the group start.
#[derive(Clone, Debug)]
pub(crate) struct ScheduledStep {
    pub(crate) at: Duration,
    pub(crate) node: NodeHandle,
    pub(crate) property: String,
    pub(crate) from: Option<Value>,
    pub(crate) to: Value,
    pub(crate) behavior: Behavior,
}

/// A group being advanced by the scene's frame tick.
pub(crate) struct RunningGroup {
    pub(crate) steps: Vec<ScheduledStep>,
    /// The order the steps start in on a backward pass, each with its start
    /// time mirrored across the pass: the step that ended last starts first.
    pub(crate) backward_order: Vec<(Duration, usize)>,
    /// Where each step actually departed from on the last forward pass, which
    /// is where a backward pass returns it to.
    pub(crate) origins: Vec<Option<Value>>,
    /// Index of the first step that has not started yet, in the order the
    /// current pass runs them.
    pub(crate) cursor: usize,
    pub(crate) elapsed: Duration,
    pub(crate) total: Duration,
    pub(crate) repeat: Repeat,
    pub(crate) passes: u32,
    pub(crate) paused: bool,
    /// Time still to wait before the first pass begins.
    pub(crate) delay: Duration,
    /// Whether this pass runs the schedule backwards.
    pub(crate) backward: bool,
}

impl RunningGroup {
    /// The start time and step index of the `cursor`th step of this pass.
    fn upcoming(&self) -> Option<(Duration, usize)> {
        if self.backward {
            self.backward_order.get(self.cursor).copied()
        } else {
            self.steps
                .get(self.cursor)
                .map(|step| (step.at, self.cursor))
        }
    }
}

/// How long one scheduled step occupies the timeline.
fn span(behavior: Behavior) -> Duration {
    behavior.delay + behavior.duration * passes(behavior)
}

/// How many times a settling repetition covers its interval.
pub(crate) fn passes(behavior: Behavior) -> u32 {
    match behavior.repeat {
        Repeat::Once => 1,
        Repeat::Times(count) | Repeat::PingPongTimes(count) => count.max(1),
        // Refused before scheduling; counted as one pass so duration stays finite.
        Repeat::Forever | Repeat::PingPong => 1,
    }
}

impl Scene {
    /// Starts a group of property animations on the frame clock.
    ///
    /// The group owns only the schedule. Each step, when its turn comes, starts
    /// an ordinary property animation, which means a write that arrives from
    /// elsewhere retargets it exactly as it would any other motion.
    pub fn start_group(
        &mut self,
        step: AnimationStep,
        repeat: Repeat,
    ) -> Result<GroupId, SceneError> {
        self.start_group_after(step, repeat, Duration::ZERO)
    }

    /// Starts a group once `delay` has passed.
    ///
    /// The wait comes once, before the first pass, not before every
    /// repetition: a stagger across several groups is what it is for, and a
    /// pause inside the schedule is already how a pass waits.
    ///
    /// An alternating repetition runs every other pass backwards: the steps
    /// start in mirrored order, each travelling from its target back to where
    /// it departed on the forward pass, with its own easing.
    pub fn start_group_after(
        &mut self,
        step: AnimationStep,
        repeat: Repeat,
        delay: Duration,
    ) -> Result<GroupId, SceneError> {
        step.validate()?;
        // A schedule with no length would restart on every tick and hold the
        // frame clock awake for nothing.
        if repeat.is_endless() && step.duration().is_zero() {
            return Err(SceneError::Reactive(
                "an endless animation group must occupy time".to_owned(),
            ));
        }
        // Resolving every property up front means a typo fails at the call that
        // started the group rather than partway through playing it.
        let mut steps = Vec::new();
        step.schedule(Duration::ZERO, &mut steps);
        for scheduled in &steps {
            self.property_key(scheduled.node, &scheduled.property)?;
        }
        steps.sort_by_key(|step| step.at);
        let total = step.duration();
        let mut backward_order = steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                let end = step.at + span(step.behavior);
                (total.saturating_sub(end), index)
            })
            .collect::<Vec<_>>();
        backward_order.sort_by_key(|(at, index)| (*at, std::cmp::Reverse(*index)));
        let id = GroupId(self.next_group);
        self.next_group = self.next_group.wrapping_add(1);
        self.groups.insert(
            id,
            RunningGroup {
                origins: vec![None; steps.len()],
                steps,
                backward_order,
                cursor: 0,
                elapsed: Duration::ZERO,
                total,
                repeat,
                passes: 0,
                paused: false,
                delay,
                backward: false,
            },
        );
        Ok(id)
    }

    /// Reports whether a group is still scheduling steps.
    pub fn is_group_active(&self, group: GroupId) -> bool {
        self.groups.contains_key(&group)
    }

    /// Halts or resumes a group's schedule.
    ///
    /// Steps already in flight keep running; pausing a group only stops it from
    /// starting the ones that come next. Pause those separately to freeze the
    /// picture entirely.
    pub fn set_group_paused(&mut self, group: GroupId, paused: bool) -> bool {
        match self.groups.get_mut(&group) {
            Some(running) => {
                running.paused = paused;
                true
            }
            None => false,
        }
    }

    /// Abandons a group's remaining steps, leaving started ones to finish.
    pub fn stop_group(&mut self, group: GroupId) -> bool {
        if self.groups.remove(&group).is_none() {
            return false;
        }
        self.group_events.push(GroupEvent {
            group,
            end: AnimationEnd::Stopped,
        });
        true
    }

    /// Runs a group to its end at once, landing every property on its target.
    pub fn finish_group(&mut self, group: GroupId) -> Result<bool, SceneError> {
        let Some(running) = self.groups.remove(&group) else {
            return Ok(false);
        };
        for step in &running.steps {
            self.assign(step.node, &step.property, step.to.clone())?;
            self.finish_animation(step.node, &step.property)?;
        }
        self.group_events.push(GroupEvent {
            group,
            end: AnimationEnd::Completed,
        });
        Ok(true)
    }

    /// Advances every group clock and starts the steps whose turn has come.
    ///
    /// A frame is not infinitely fine, so a step's start time usually falls
    /// partway through one. The remainder of that frame is handed to the step as
    /// tween delay, which the clock drains before it begins advancing. The step
    /// therefore ends the frame at exactly the progress its start time earned,
    /// and a long sequence does not drift a frame further out with every leg.
    pub(crate) fn tick_groups(&mut self, delta: Duration) -> Result<Vec<GroupEvent>, SceneError> {
        let mut events = std::mem::take(&mut self.group_events);
        let mut due = Vec::new();
        let mut finished = Vec::new();
        for (id, group) in &mut self.groups {
            if group.paused {
                continue;
            }
            // The group's own delay drains first; whatever of this frame is
            // left over is the first pass's.
            let frame = delta;
            let mut delta = delta;
            if !group.delay.is_zero() {
                let waited = group.delay.min(delta);
                group.delay -= waited;
                delta -= waited;
                if !group.delay.is_zero() {
                    continue;
                }
            }
            group.elapsed += delta;
            while let Some((at, index)) = group.upcoming() {
                if at > group.elapsed {
                    break;
                }
                let mut step = group.steps[index].clone();
                // Measured against the whole frame, which is what the step's own
                // clock advances by on this tick, the group's delay included.
                let lag = frame.saturating_sub(group.elapsed - at);
                if group.backward {
                    // Back from its target to where it set out from, with no
                    // lead-in: a step's delay on the way out is the idle time
                    // after it on the way back.
                    step.from = Some(step.to.clone());
                    step.to = group.origins[index]
                        .clone()
                        .unwrap_or_else(|| step.to.clone());
                    step.behavior.delay = lag;
                } else {
                    step.behavior.delay += lag;
                }
                due.push((*id, index, group.backward, step));
                group.cursor += 1;
            }
            if group.elapsed < group.total {
                continue;
            }
            group.passes += 1;
            let (repeating, alternating) = match group.repeat {
                Repeat::Forever => (true, false),
                Repeat::Times(count) => (group.passes < count.max(1), false),
                Repeat::PingPong => (true, true),
                Repeat::PingPongTimes(count) => (group.passes < count.max(1), true),
                Repeat::Once => (false, false),
            };
            if repeating {
                group.cursor = 0;
                group.elapsed = Duration::ZERO;
                if alternating {
                    group.backward = !group.backward;
                }
            } else {
                finished.push(*id);
            }
        }
        for (id, index, backward, step) in due {
            // Without an explicit start the step behaves like a write through
            // an installed behavior: it departs from wherever the property
            // currently sits.
            let from = match &step.from {
                Some(from) => from.clone(),
                None => self.current(step.node, &step.property)?.clone(),
            };
            if !backward && let Some(group) = self.groups.get_mut(&id) {
                group.origins[index] = Some(from.clone());
            }
            self.animate_from(step.node, &step.property, from, step.to, step.behavior)?;
        }
        for id in finished {
            self.groups.remove(&id);
            events.push(GroupEvent {
                group: id,
                end: AnimationEnd::Completed,
            });
        }
        Ok(events)
    }

    /// Drops groups whose target nodes are gone, reporting each as cancelled.
    ///
    /// The event matters as much as the removal: a caller waiting on a group
    /// that can no longer run needs to hear that it ended, and any handler
    /// registered for it has to be released.
    pub(crate) fn retain_live_groups(&mut self) {
        let nodes = &self.nodes;
        let events = &mut self.group_events;
        self.groups.retain(|id, group| {
            let live = group
                .steps
                .iter()
                .all(|step| nodes.contains_key(step.node.0));
            if !live {
                events.push(GroupEvent {
                    group: *id,
                    end: AnimationEnd::Canceled,
                });
            }
            live
        });
    }
}
