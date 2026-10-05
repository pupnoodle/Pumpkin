use std::sync::atomic::Ordering;

use super::{Controls, Goal};
use crate::entity::mob::Mob;
use pumpkin_data::entity::EntityPose;

/// Vanilla keeps a sleeping mob in bed: while the pose is `Sleeping` this
/// goal owns the move control, so no movement goal can run until it wakes.
#[derive(Default)]
pub struct SleepGoal {
    goal_control: Controls,
}

impl SleepGoal {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            goal_control: Controls::MOVE,
        }
    }
}

impl Goal for SleepGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        mob.get_entity().pose.load() == EntityPose::Sleeping
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.can_start(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity()
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop();
    }

    fn controls(&self) -> Controls {
        self.goal_control
    }
}
