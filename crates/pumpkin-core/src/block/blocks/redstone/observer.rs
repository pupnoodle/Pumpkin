use std::sync::Arc;

use crate::block::{
    EmitsRedstonePowerArgs, GetRedstonePowerArgs, GetStateForNeighborUpdateArgs,
    OnNeighborUpdateArgs, OnPlaceArgs, OnScheduledTickArgs, OnStateReplacedArgs,
};
use crate::entity::EntityBase;
use pumpkin_data::{
    Block, BlockDirection, BlockStateId, FacingExt,
    block_properties::{
        MovingPistonLikeProperties, ObserverLikeProperties, PistonHeadLikeProperties,
    },
};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::{tick::TickPriority, world::BlockFlags};

use crate::{block::BlockBehaviour, world::World};

const PULSE_TICKS: u8 = 2;

#[pumpkin_block("minecraft:observer")]
pub struct ObserverBlock;

impl BlockBehaviour for ObserverBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let mut props = ObserverLikeProperties::default(args.block);
        props.facing = args.player.get_entity().get_facing();
        props.to_state_id(args.block)
    }

    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        let state = args.world.get_block_state(args.position);
        let mut props = ObserverLikeProperties::from_state_id(state.id);
        let step = step_pulse(props.powered);
        props.powered = step.powered;
        args.world.set_block_state(
            args.position,
            props.to_state_id(args.block),
            BlockFlags::NOTIFY_LISTENERS,
        );
        if step.schedule {
            args.world.schedule_block_tick(
                args.block,
                *args.position,
                PULSE_TICKS,
                TickPriority::Normal,
            );
        }
        Self::update_neighbors(args.world, args.block, args.position, props);
    }

    fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
        let state = args.world.get_block_state(args.position);
        let props = ObserverLikeProperties::from_state_id(state.id);
        let facing = props.facing.to_block_direction();
        if should_arm(
            props.powered,
            args.world
                .is_block_tick_scheduled(args.position, &Block::OBSERVER),
            neighbor_update_watches_face(args.world, args.position, facing, args.source_block),
        ) {
            Self::schedule_tick(args.world, args.position);
        }
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        let props = ObserverLikeProperties::from_state_id(args.state_id);
        let facing = props.facing.to_block_direction();
        if should_arm(
            props.powered,
            args.world
                .is_block_tick_scheduled(args.position, &Block::OBSERVER),
            watches(facing, args.direction),
        ) {
            Self::schedule_tick(args.world, args.position);
        }

        args.state_id
    }

    fn emits_redstone_power(&self, _args: EmitsRedstonePowerArgs<'_>) -> bool {
        true
    }

    fn get_weak_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        let props = ObserverLikeProperties::from_state_id(args.state.id);
        power_level(
            props.facing.to_block_direction(),
            args.direction,
            props.powered,
        )
    }

    fn get_strong_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        self.get_weak_redstone_power(args)
    }

    fn on_state_replaced(&self, args: OnStateReplacedArgs<'_>) {
        if !args.moved {
            let props = ObserverLikeProperties::from_state_id(args.old_state_id);
            if props.powered
                && args
                    .world
                    .is_block_tick_scheduled(args.position, &Block::OBSERVER)
            {
                Self::update_neighbors(args.world, args.block, args.position, props);
            }
        }
    }
}

impl ObserverBlock {
    fn update_neighbors(
        world: &Arc<World>,
        block: &Block,
        block_pos: &BlockPos,
        props: ObserverLikeProperties,
    ) {
        let facing = props.facing.to_block_direction();
        let opposite_facing_pos = block_pos.offset(facing.opposite().to_offset());

        world.update_neighbor(&opposite_facing_pos, block);
        world.update_neighbors(&opposite_facing_pos, Some(facing));
    }

    fn schedule_tick(world: &World, block_pos: &BlockPos) {
        world.schedule_block_tick(
            &Block::OBSERVER,
            *block_pos,
            PULSE_TICKS,
            TickPriority::Normal,
        );
    }
}

struct PulseStep {
    powered: bool,
    schedule: bool,
}

const fn step_pulse(powered: bool) -> PulseStep {
    if powered {
        PulseStep {
            powered: false,
            schedule: false,
        }
    } else {
        PulseStep {
            powered: true,
            schedule: true,
        }
    }
}

const fn should_arm(powered: bool, tick_scheduled: bool, watched_face: bool) -> bool {
    watched_face && !powered && !tick_scheduled
}

fn watches(facing: BlockDirection, neighbor_direction: BlockDirection) -> bool {
    facing == neighbor_direction
}

const fn output_face(facing: BlockDirection) -> BlockDirection {
    facing.opposite()
}

fn power_level(facing: BlockDirection, queried_from: BlockDirection, powered: bool) -> u8 {
    if powered && facing == queried_from {
        15
    } else {
        0
    }
}

fn neighbor_update_watches_face(
    world: &World,
    pos: &BlockPos,
    facing: BlockDirection,
    source: &Block,
) -> bool {
    source == &Block::MOVING_PISTON && piston_arrived_on_watched_face(world, pos, facing)
}

fn piston_arrived_on_watched_face(world: &World, pos: &BlockPos, facing: BlockDirection) -> bool {
    let front = pos.offset(facing.to_offset());
    points_into_watched_face(world, &front, facing)
        || points_into_watched_face(world, &front.offset(facing.to_offset()), facing)
}

fn points_into_watched_face(
    world: &World,
    pos: &BlockPos,
    observer_facing: BlockDirection,
) -> bool {
    let (block, state) = world.get_block_and_state(pos);
    let piston_facing = if block == &Block::PISTON_HEAD {
        PistonHeadLikeProperties::from_state_id(state.id).facing
    } else if block == &Block::MOVING_PISTON {
        MovingPistonLikeProperties::from_state_id(state.id).facing
    } else {
        return false;
    };
    piston_faces_into(observer_facing, piston_facing.to_block_direction())
}

fn piston_faces_into(observer_facing: BlockDirection, piston_facing: BlockDirection) -> bool {
    piston_facing == output_face(observer_facing)
}

#[cfg(test)]
mod tests {
    use super::{
        PULSE_TICKS, output_face, piston_faces_into, power_level, should_arm, step_pulse, watches,
    };
    use pumpkin_data::BlockDirection;

    #[test]
    fn pulse_arms_for_the_watched_face_and_not_the_output() {
        let facing = BlockDirection::North;
        assert!(watches(facing, facing));
        assert!(!watches(facing, output_face(facing)));
        assert_ne!(output_face(facing), facing);
        for side in [
            BlockDirection::East,
            BlockDirection::West,
            BlockDirection::Up,
            BlockDirection::Down,
        ] {
            assert!(!watches(facing, side));
        }
    }

    #[test]
    fn pulse_turns_on_then_off_after_two_ticks() {
        assert_eq!(PULSE_TICKS, 2);
        let on = step_pulse(false);
        assert!(on.powered);
        assert!(on.schedule);
        let off = step_pulse(true);
        assert!(!off.powered);
        assert!(!off.schedule);
    }

    #[test]
    fn already_powered_or_scheduled_observer_does_not_arm_again() {
        assert!(should_arm(false, false, true));
        assert!(!should_arm(true, false, true));
        assert!(!should_arm(false, true, true));
        assert!(!should_arm(false, false, false));
    }

    #[test]
    fn piston_entering_the_watched_face_does_not_count_as_output() {
        let facing = BlockDirection::North;
        assert!(piston_faces_into(facing, output_face(facing)));
        assert!(!piston_faces_into(facing, facing));
    }

    #[test]
    fn output_neighbor_reads_power_and_the_watched_face_does_not() {
        let facing = BlockDirection::East;
        assert_eq!(power_level(facing, facing, true), 15);
        assert_eq!(power_level(facing, output_face(facing), true), 0);
        assert_eq!(power_level(facing, facing, false), 0);
    }
}
